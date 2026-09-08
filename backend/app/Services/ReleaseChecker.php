<?php

declare(strict_types=1);

namespace App\Services;

use DateTimeImmutable;
use Filebeam\Updater\Catalog;
use Filebeam\Updater\Semver;
use Illuminate\Http\Client\ConnectionException;
use Illuminate\Http\Client\RequestException;
use Illuminate\Support\Facades\Http;
use Illuminate\Support\Facades\Process;
use JsonException;
use Random\RandomException;
use RuntimeException;
use Symfony\Component\Process\PhpExecutableFinder;
use Throwable;

class ReleaseChecker
{
    private const string CatalogUrl = 'https://releases.filebeam.io/index.json';

    /** @return array<string, mixed>
     * @throws JsonException
     */
    public function updaterStatus(): array
    {
        return $this->read('status.json') ?? ['state' => 'idle'];
    }

    /** @return array<string, mixed>|null
     * @throws JsonException
     */
    private function read(string $name): ?array
    {
        $file = $this->path($name);
        if (! is_file($file)) {
            return null;
        }

        $contents = file_get_contents($file);
        if ($contents === false || strlen($contents) > 1048576) {
            throw new RuntimeException('Unable to read update state file.');
        }
        $value = json_decode($contents, true, 64, JSON_THROW_ON_ERROR);

        return is_array($value) ? $value : throw new RuntimeException('Invalid update state file.');
    }

    private function path(string $name): string
    {
        $directory = (string) config('filebeam.updates.state_path');
        if (! is_dir($directory) && ! mkdir($directory, 0700, true) && ! is_dir($directory)) {
            throw new RuntimeException('Cannot create updater state directory.');
        }

        chmod($directory, 0700);

        return $directory.'/'.$name;
    }

    /** @return array<string, mixed>
     * @throws JsonException
     */
    public function checkAndInstallAutomaticUpdate(): array
    {
        $state = $this->check();
        $release = $state['latest'] ?? null;
        $driver = config('database.connections.'.config('database.default').'.driver');
        if (! config('filebeam.updates.auto_enabled')
            || ! in_array($driver, ['sqlite', 'mysql', 'mariadb', 'pgsql'], true)
            || ($state['state'] ?? null) !== 'available'
            || ! is_array($release)
            || ! is_string($release['tag'] ?? null)
            || ! ($release['upgradeable'] ?? false)) {
            return $state;
        }

        if (! $this->availability(requireCron: false)['available']) {
            return $state;
        }

        Process::path($this->rootPath())
            ->timeout(900)
            ->run([$this->phpBinary(), 'update.php', $release['tag']])
            ->throw();

        return $state;
    }

    /** @return array<string, mixed> */
    public function check(): array
    {
        try {
            $previous = $this->state();
        } catch (Throwable $exception) {
            return [
                'state' => 'error',
                'checked_at' => now('UTC')->toIso8601String(),
                'latest' => null,
                'error' => 'Unable to safely read release check state: '.$exception->getMessage(),
            ];
        }

        try {
            $catalog = $this->catalog($previous);
            $latest = $this->latestRelease($catalog);
            $state = [
                'state' => $latest === null ? 'current' : 'available',
                'checked_at' => now('UTC')->toIso8601String(),
                'latest' => $latest,
                'error' => null,
                'generation' => $catalog['generation'],
            ];
        } catch (Throwable $exception) {
            $state = [
                ...$previous,
                'state' => 'error',
                'checked_at' => now('UTC')->toIso8601String(),
                'error' => $exception->getMessage(),
            ];
        }

        $this->write('release-check.json', $state);

        return $state;
    }

    /** @return array<string, mixed>
     * @throws JsonException
     */
    public function state(): array
    {
        return $this->read('release-check.json') ?? [
            'state' => 'unchecked',
            'checked_at' => null,
            'latest' => null,
            'error' => null,
        ];
    }

    /**
     * @param  array<string, mixed>  $previous
     * @return array{schema: int, generation: int, releases: list<mixed>, expires_at: string, published_at?: string}
     *
     * @throws JsonException
     * @throws RequestException|ConnectionException
     */
    private function catalog(array $previous): array
    {
        if (! extension_loaded('sodium')) {
            throw new RuntimeException('The sodium PHP extension is required.');
        }
        $key = base64_decode((string) config('version.update_public_key'), true);
        if ($key === false || strlen($key) !== SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES) {
            throw new RuntimeException('This installation has no valid update signing key.');
        }

        $response = Http::acceptJson()->timeout(120)->connectTimeout(15)->get(self::CatalogUrl);
        $response->throw();
        $catalog = Catalog::verify($response->body(), $key, now('UTC')->getTimestamp());

        if (isset($previous['generation']) && ! is_int($previous['generation'])) {
            throw new RuntimeException('Stored release catalog generation is invalid.');
        }
        if ($catalog['generation'] < ($previous['generation'] ?? 0)) {
            throw new RuntimeException('Release catalog rollback detected.');
        }

        $normalizedCatalog = [
            'schema' => $catalog['schema'],
            'generation' => $catalog['generation'],
            'expires_at' => $catalog['expires_at'],
            'releases' => array_values($catalog['releases']),
        ];
        if (is_string($catalog['published_at'] ?? null)) {
            $normalizedCatalog['published_at'] = $catalog['published_at'];
        }

        return $normalizedCatalog;
    }

    /** @param array{schema: int, generation: int, releases: list<mixed>, expires_at: string, published_at?: string} $catalog
     * @return array<string, mixed>|null
     */
    private function latestRelease(array $catalog): ?array
    {
        $current = (string) config('version.version');
        $candidates = [];
        foreach ($catalog['releases'] as $release) {
            if (is_array($release) && $this->validRelease($release)) {
                $candidates[] = $release;
            }
        }
        usort($candidates, fn (array $left, array $right): int => Semver::compare((string) $right['version'], (string) $left['version']));
        $release = $candidates[0] ?? null;

        if (! is_array($release) || Semver::compare((string) $release['version'], $current) <= 0) {
            return null;
        }

        $minimumVersion = (string) ($release['minimum_version'] ?? '0.0.0');
        $upgradeable = ($release['minimum_updater'] ?? 1) <= 1 && Semver::compare($current, $minimumVersion) >= 0;

        return [
            'tag' => $release['tag'],
            'version' => $release['version'],
            'upgradeable' => $upgradeable,
            'security_warnings' => array_values(array_filter($release['security_warnings'] ?? [], 'is_string')),
            'warning' => $upgradeable ? null : 'This release requires a newer intermediate Filebeam version or updater.',
        ];
    }

    /** @param array<string, mixed> $release */
    private function validRelease(array $release): bool
    {
        $tag = Semver::parseTag((string) ($release['tag'] ?? ''), true);
        $version = Semver::parseTag((string) ($release['version'] ?? ''), true);

        return $tag !== null
            && $version === $tag
            && ! str_contains($version, '-')
            && ! ($release['withdrawn'] ?? false)
            && is_array($release['package'] ?? null);
    }

    /** @param array<string, mixed> $value
     * @throws RandomException
     * @throws JsonException
     */
    private function write(string $name, array $value): void
    {
        $file = $this->path($name);
        $temporary = $file.'.'.bin2hex(random_bytes(8)).'.tmp';
        $contents = json_encode($value, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
        if (file_put_contents($temporary, $contents, LOCK_EX) === false || ! chmod($temporary, 0600) || ! rename($temporary, $file)) {
            @unlink($temporary);
            throw new RuntimeException('Unable to write update state file.');
        }
    }

    /** @return array<string, mixed>
     * @throws JsonException
     */
    public function availability(bool $requireCron = true): array
    {
        $version = config('version');
        $reasons = [];

        if (($version['distribution'] ?? null) !== 'package') {
            $reasons[] = 'Self-updates are available only for package installations.';
        }

        if (is_file('/.dockerenv')) {
            $reasons[] = 'Self-updates are unavailable in containers.';
        }

        if (! extension_loaded('sodium') || ! extension_loaded('zip') || ! function_exists('curl_init')) {
            $reasons[] = 'The sodium, zip, and curl PHP extensions are required.';
        } elseif (! $this->validPublicKey((string) ($version['update_public_key'] ?? ''))) {
            $reasons[] = 'This installation has no valid update signing key.';
        }

        if (! is_file($this->rootPath().'/update.php') || ! is_readable($this->rootPath().'/update.php') || ! is_executable($this->phpBinary())) {
            $reasons[] = 'The command-line updater is not executable.';
        }

        if ($requireCron && ! $this->cronIsReady()) {
            $reasons[] = 'The updater cron has not reported a heartbeat in the last 24 hours.';
        }

        if ($reasons === []
            && config('database.connections.'.config('database.default').'.driver') === 'pgsql'
            && ! $this->postgresBackupIsReady()) {
            $reasons[] = 'PostgreSQL updates require matching pg_dump/pg_restore and a reachable database. Run php update.php --check-backup.';
        }

        return ['available' => $reasons === [], 'reasons' => $reasons];
    }

    private function postgresBackupIsReady(): bool
    {
        try {
            return Process::path($this->rootPath())
                ->timeout(45)
                ->run([$this->phpBinary(), 'update.php', '--check-backup'])
                ->successful();
        } catch (Throwable) {
            return false;
        }
    }

    private function validPublicKey(string $encoded): bool
    {
        $key = base64_decode($encoded, true);

        return $key !== false && strlen($key) === SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES;
    }

    private function rootPath(): string
    {
        return dirname(base_path());
    }

    private function phpBinary(): string
    {
        return (new PhpExecutableFinder)->find(false) ?: 'php';
    }

    /**
     * @throws JsonException
     */
    private function cronIsReady(): bool
    {
        $heartbeat = $this->updaterHeartbeat();
        if (! is_string($heartbeat['at'] ?? null)) {
            return false;
        }

        try {
            return new DateTimeImmutable($heartbeat['at']) >= now('UTC')->subDay();
        } catch (Throwable) {
            return false;
        }
    }

    /** @return array<string, mixed>|null
     * @throws JsonException
     */
    public function updaterHeartbeat(): ?array
    {
        return $this->read('heartbeat.json');
    }

    /**
     * @throws RandomException
     * @throws JsonException
     */
    public function queue(string $tag, string $requestedBy): void
    {
        $availability = $this->availability();
        if (! $availability['available']) {
            throw new RuntimeException(implode(' ', $availability['reasons']));
        }

        $state = $this->state();
        $release = $state['latest'] ?? null;
        if (($state['state'] ?? null) !== 'available') {
            throw new RuntimeException('Run a successful release check before queuing an upgrade.');
        }
        if (! is_array($release) || ($release['tag'] ?? null) !== $tag || ! ($release['upgradeable'] ?? false)) {
            throw new RuntimeException('The requested release is not available for this installation.');
        }

        $this->withLock(/**
         * @throws RandomException
         * @throws JsonException
         */ function () use ($tag, $requestedBy): void {
            $this->write('pending.json', [
                'tag' => $tag,
                'requested_by' => $requestedBy,
                'requested_at' => now('UTC')->toIso8601String(),
            ]);
        });
    }

    private function withLock(callable $callback): void
    {
        $lock = fopen($this->path('updater.lock'), 'c+') ?: throw new RuntimeException('Cannot open updater lock.');
        if (! flock($lock, LOCK_EX | LOCK_NB)) {
            fclose($lock);
            throw new RuntimeException('Another updater is running.');
        }

        try {
            $callback();
        } finally {
            flock($lock, LOCK_UN);
            fclose($lock);
        }
    }
}
