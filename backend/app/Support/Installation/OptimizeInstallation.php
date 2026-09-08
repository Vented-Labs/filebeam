<?php

declare(strict_types=1);

namespace App\Support\Installation;

use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\Process;
use Illuminate\Validation\ValidationException;
use Symfony\Component\Process\ExecutableFinder;
use Throwable;

class OptimizeInstallation
{
    public function __construct(
        private readonly ?string $basePath = null,
    ) {}

    public function handle(): void
    {
        $this->clearCachedConfiguration();

        try {
            $result = Process::path($this->basePath())
                ->env($this->environment())
                ->timeout(300)
                ->run([$this->phpBinary(), 'artisan', 'optimize', '--no-interaction']);
        } catch (Throwable) {
            $this->clearPartialCaches();

            throw ValidationException::withMessages(['installation' => 'Application optimization could not be completed. Please retry.']);
        }

        if (! $result->successful() || str_contains($result->output(), 'FAIL') || ! $this->hasExpectedArtifacts()) {
            $this->clearPartialCaches();

            throw ValidationException::withMessages(['installation' => 'Application optimization could not be completed. Please retry.']);
        }
    }

    private function clearCachedConfiguration(): void
    {
        $path = $this->cachePaths()['config'];

        if (File::exists($path) && ! File::delete($path)) {
            throw ValidationException::withMessages(['installation' => 'Runtime configuration cache could not be cleared.']);
        }
    }

    /** @return array{config: string, routes: string, events: string} */
    private function cachePaths(): array
    {
        /** @var array<string, string> $externalEnvironment */
        $externalEnvironment = app('installation.external_environment');

        return [
            'config' => $this->cachePath('APP_CONFIG_CACHE', 'config.php', $externalEnvironment, app()->getCachedConfigPath()),
            'routes' => $this->cachePath('APP_ROUTES_CACHE', 'routes-v7.php', $externalEnvironment, app()->getCachedRoutesPath()),
            'events' => $this->cachePath('APP_EVENTS_CACHE', 'events.php', $externalEnvironment, app()->getCachedEventsPath()),
        ];
    }

    /** @param array<string, string> $externalEnvironment */
    private function cachePath(string $key, string $default, array $externalEnvironment, string $applicationPath): string
    {
        if (! array_key_exists($key, $externalEnvironment)) {
            return $this->basePath === null ? $applicationPath : $this->bootstrapPath('cache/'.$default);
        }

        if ($this->basePath === null) {
            return $applicationPath;
        }

        $path = $externalEnvironment[$key];

        return str_starts_with($path, DIRECTORY_SEPARATOR) || preg_match('/^[A-Za-z]:[\\\\\/]/', $path) === 1
            ? $path
            : $this->basePath($path);
    }

    private function bootstrapPath(string $path = ''): string
    {
        return $this->basePath === null ? app()->bootstrapPath($path) : $this->basePath('bootstrap'.DIRECTORY_SEPARATOR.$path);
    }

    private function basePath(string $path = ''): string
    {
        if ($this->basePath === null) {
            return base_path($path);
        }

        return $path === '' ? $this->basePath : $this->basePath.DIRECTORY_SEPARATOR.$path;
    }

    /** @return array<string, string|false> */
    private function environment(): array
    {
        $environment = array_filter(getenv(), is_string(...));

        foreach ([...EnvironmentSettings::outputKeys(), ...EnvironmentSettings::cachePathKeys()] as $key) {
            $environment[$key] = false;
        }

        /** @var array<string, string> $externalEnvironment */
        $externalEnvironment = app('installation.external_environment');

        return array_replace($environment, $externalEnvironment);
    }

    private function phpBinary(): string
    {
        $configuredBinary = config('installation.php_binary');

        if (is_string($configuredBinary) && $configuredBinary !== '' && is_executable($configuredBinary)) {
            return $configuredBinary;
        }

        return (new ExecutableFinder)->find('php') ?: 'php';
    }

    private function clearPartialCaches(): void
    {
        foreach ($this->cachePaths() as $path) {
            File::delete($path);
        }

        File::delete($this->bootstrapPath('cache/blade-icons.php'));
        File::deleteDirectory($this->bootstrapPath('cache/filament'));
        $views = $this->basePath === null
            ? (string) config('view.compiled', $this->storagePath('framework/views'))
            : $this->storagePath('framework/views');
        File::cleanDirectory($views);
    }

    private function storagePath(string $path = ''): string
    {
        return $this->basePath === null ? storage_path($path) : $this->basePath('storage'.DIRECTORY_SEPARATOR.$path);
    }

    private function hasExpectedArtifacts(): bool
    {
        return array_all($this->cachePaths(), fn ($path) => File::exists($path));
    }
}
