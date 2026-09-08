<?php

declare(strict_types=1);

namespace App\Support\Installation;

use Closure;
use Illuminate\Contracts\Filesystem\FileNotFoundException;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Str;
use Illuminate\Validation\ValidationException;
use JsonException;
use Random\RandomException;
use RuntimeException;
use Throwable;

class InstallationState
{
    public function isCompleted(): bool
    {
        return ($this->read()['status'] ?? null) === 'completed';
    }

    /** @return array<string, mixed>|null */
    public function read(): ?array
    {
        $path = $this->directory().'/state.json';
        if (! file_exists($path)) {
            return null;
        }

        try {
            $state = json_decode(File::get($path), true, 32, JSON_THROW_ON_ERROR);
            if (is_array($state) && is_string($state['id'] ?? null) && in_array($state['status'] ?? null, ['pending', 'installing', 'completed'], true)) {
                return $state;
            }
        } catch (Throwable) {
            // An unreadable marker must never turn an existing installation into a fresh one.
        }

        return ['status' => 'blocked'];
    }

    public function directory(): string
    {
        return (string) config('installation.state_directory');
    }

    public function requiresSetup(): bool
    {
        $state = $this->read();

        return $state !== null ? ($state['status'] ?? null) !== 'completed' : ! file_exists($this->environmentPath());
    }

    public function environmentPath(): string
    {
        return (string) config('installation.environment_path');
    }

    public function challenge(): string
    {
        return $this->locked(function (): string {
            $path = $this->directory().'/bootstrap.key';
            if (! file_exists($path)) {
                $handle = fopen($path, 'x');
                if ($handle === false) {
                    throw new RuntimeException('Bootstrap challenge could not be initialized.');
                }
                try {
                    chmod($path, 0600);
                    32
                        |> random_bytes(...)
                        |> bin2hex(...)
                        |> (fn ($x) => fwrite($handle, $x));
                } finally {
                    fclose($handle);
                }
            }
            $expires = (string) (time() + 900);

            return $expires.'.'.hash_hmac('sha256', $expires, File::get($path));
        });
    }

    public function locked(Closure $callback): mixed
    {
        $this->ensureDirectory();
        $handle = fopen($this->directory().'/installation.lock', 'c');
        if ($handle === false) {
            throw new RuntimeException('Installation lock could not be opened.');
        }
        try {
            if (! flock($handle, LOCK_EX | LOCK_NB)) {
                throw ValidationException::withMessages(['installation' => 'Another installation operation is in progress. Please retry shortly.']);
            }

            return $callback();
        } finally {
            flock($handle, LOCK_UN);
            fclose($handle);
        }
    }

    private function ensureDirectory(): void
    {
        File::ensureDirectoryExists($this->directory(), 0700, true);
    }

    /**
     * @throws FileNotFoundException
     */
    public function validChallenge(string $challenge): bool
    {
        $parts = explode('.', $challenge);
        $path = $this->directory().'/bootstrap.key';

        return count($parts) === 2 && ctype_digit($parts[0]) && (int) $parts[0] >= time()
            && (int) $parts[0] <= time() + 900 && is_file($path)
            && hash_equals(hash_hmac('sha256', $parts[0], File::get($path)), $parts[1]);
    }

    public function bootstrap(EnvironmentWriter $writer): void
    {
        $this->locked(/**
         * @throws RandomException
         */ function () use ($writer): void {
            if ($this->isPending() && file_exists($this->environmentPath())) {
                return;
            }
            abort_unless($this->canBootstrap(), 404);
            $token = bin2hex(random_bytes(32));
            $this->write(['id' => (string) Str::uuid(), 'status' => 'pending', 'token_hash' => hash('sha256', $token)]);
            $writer->write([
                'APP_ENV' => 'production',
                'APP_DEBUG' => 'false',
                'APP_KEY' => $this->existingApplicationKey() ?? 'base64:'.base64_encode(random_bytes(32)),
                'APP_NAME' => 'Filebeam',
                'APP_URL' => 'http://localhost',
                'SESSION_DRIVER' => 'cookie',
                'CACHE_STORE' => config('installation.container') && config('installation.container_variant') === 'omnibus' ? 'redis' : 'file',
                'QUEUE_CONNECTION' => config('installation.container') && config('installation.container_variant') === 'omnibus' ? 'redis' : 'database',
                'FILEBEAM_CRON_QUEUE_ENABLED' => config('installation.container') ? 'false' : null,
                'REDIS_CLIENT' => config('installation.container') && config('installation.container_variant') === 'omnibus' ? 'phpredis' : null,
                'REDIS_HOST' => config('installation.container') && config('installation.container_variant') === 'omnibus' ? '/run/filebeam/valkey/valkey.sock' : null,
                'REDIS_PORT' => config('installation.container') && config('installation.container_variant') === 'omnibus' ? '0' : null,
                'REDIS_DB' => config('installation.container') && config('installation.container_variant') === 'omnibus' ? '0' : null,
                'REDIS_CACHE_DB' => config('installation.container') && config('installation.container_variant') === 'omnibus' ? '1' : null,
                'REDIS_CACHE_CONNECTION' => config('installation.container') && config('installation.container_variant') === 'omnibus' ? 'cache' : null,
                'REDIS_CACHE_LOCK_CONNECTION' => config('installation.container') && config('installation.container_variant') === 'omnibus' ? 'cache' : null,
                'FILEBEAM_INSTALL_TOKEN' => $token,
            ], createOnly: true);
            $this->writeGeneration();
        });
    }

    public function isPending(): bool
    {
        return in_array($this->read()['status'] ?? null, ['pending', 'installing'], true);
    }

    public function canBootstrap(): bool
    {
        if (file_exists($this->environmentPath()) || is_link($this->environmentPath())) {
            return false;
        }
        $state = $this->read();
        if ($state !== null) {
            return ($state['status'] ?? null) === 'pending';
        }

        // Missing configuration is not evidence of a new deployment when local data remains.
        $sqlite = (string) config('installation.sqlite_directory').'/database.sqlite';

        return (! config('installation.container') || ! app()->configurationIsCached())
            && ! (is_file($sqlite) && filesize($sqlite) > 0)
            && (config('installation.container') || ! (app('installation.external_environment')['APP_KEY'] ?? null));
    }

    private function existingApplicationKey(): ?string
    {
        $key = app('installation.external_environment')['APP_KEY'] ?? null;

        return is_string($key) && $key !== '' ? $key : null;
    }

    /** @param array<string, mixed> $state
     * @throws RandomException
     */
    public function write(array $state): void
    {
        $this->ensureDirectory();
        $path = $this->directory().'/state.json';
        $temporary = $path.'.'.bin2hex(random_bytes(8));
        $handle = fopen($temporary, 'x');
        if ($handle === false) {
            throw new RuntimeException('Installation state could not be saved.');
        }
        try {
            $contents = json_encode($state, JSON_THROW_ON_ERROR);
            if (! chmod($temporary, 0600) || fwrite($handle, $contents) !== strlen($contents) || ! fflush($handle) || ! fsync($handle)) {
                throw new RuntimeException('Installation state could not be saved.');
            }
            if (! rename($temporary, $path)) {
                throw new RuntimeException('Installation state could not be saved.');
            }
        } catch (JsonException $e) {
        } finally {
            fclose($handle);
            if (file_exists($temporary)) {
                unlink($temporary);
            }
        }
    }

    public function writeGeneration(): void
    {
        $this->ensureDirectory();
        $path = $this->directory().'/runtime.generation';
        $generation = is_file($path) ? (int) File::get($path) : 0;
        $temporary = $path.'.'.bin2hex(random_bytes(8));
        $contents = (string) ($generation + 1)."\n";
        if (file_put_contents($temporary, $contents, LOCK_EX) !== strlen($contents) || ! chmod($temporary, 0600) || ! rename($temporary, $path)) {
            @unlink($temporary);
            throw new RuntimeException('Runtime generation could not be saved.');
        }
    }
}
