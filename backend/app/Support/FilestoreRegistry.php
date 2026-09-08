<?php

declare(strict_types=1);

namespace App\Support;

use App\Models\Filestore;
use App\Models\Plan;
use Illuminate\Contracts\Filesystem\Filesystem;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\Storage;
use Illuminate\Validation\ValidationException;
use InvalidArgumentException;
use Throwable;

class FilestoreRegistry
{
    public function environmentManaged(): bool
    {
        return $this->environmentDisks() !== null;
    }

    /** @return array<int, string>|null */
    public function environmentDisks(): ?array
    {
        $disks = config('filebeam.filesystems.environment');

        if ($disks === null) {
            return null;
        }

        if (! is_array($disks) || $disks === [] || array_any($disks, fn (mixed $disk): bool => ! is_string($disk) || $disk === '')) {
            throw new InvalidArgumentException('FILEBEAM_FILESYSTEMS must be a comma-separated list of disk names.');
        }

        return $disks;
    }

    public function disk(Filestore $store): Filesystem
    {
        if (in_array($store->source, ['environment', 'laravel'], true)) {
            if (! is_string($store->disk_name) || $store->disk_name === '') {
                throw new InvalidArgumentException('A Laravel filestore requires a disk name.');
            }

            return $this->laravelDisk($store->disk_name);
        }

        if ($store->source !== 'database') {
            throw new InvalidArgumentException('Unsupported filestore source.');
        }

        $configuration = $this->databaseConfiguration($store);
        try {
            return Storage::build($configuration);
        } catch (Throwable) {
            throw new InvalidArgumentException('The database filestore could not be initialized.');
        }
    }

    private function laravelDisk(string $name): Filesystem
    {
        $configuration = config("filesystems.disks.{$name}");
        if ($configuration !== null) {
            if (! is_array($configuration) || ! is_string($configuration['driver'] ?? null) || $configuration['driver'] === '' || ($configuration['visibility'] ?? null) !== 'private') {
                throw new InvalidArgumentException('Laravel filestores must use a configured private disk.');
            }
        }

        try {
            return Storage::disk($name);
        } catch (Throwable) {
            throw new InvalidArgumentException('The Laravel filesystem disk is not available.');
        }
    }

    /** @return array<string, mixed> */
    private function databaseConfiguration(Filestore $store): array
    {
        $configuration = $store->getAttribute('configuration');
        $driver = $store->getAttribute('driver');

        if (! is_array($configuration) || ! is_string($driver)) {
            throw new InvalidArgumentException('Database filestores require a driver and configuration.');
        }

        if (! in_array($driver, ['local', 's3'], true)) {
            throw new InvalidArgumentException('The filestore driver is not supported.');
        }

        if (($configuration['visibility'] ?? 'private') !== 'private') {
            throw new InvalidArgumentException('Database filestores must be private.');
        }

        if ($driver === 'local') {
            $root = $configuration['root'] ?? null;
            if (! is_string($root)) {
                throw new InvalidArgumentException('Local filestore roots must be relative to FILEBEAM_FILESTORE_ROOT.');
            }

            $configuration['root'] = $this->localRoot($root);
        }

        if ($driver === 's3' && (! is_string($configuration['bucket'] ?? null) || $configuration['bucket'] === '')) {
            throw new InvalidArgumentException('S3 filestores require a bucket.');
        }

        if ($driver === 's3') {
            $configuration['http'] = ['connect_timeout' => 10, 'timeout' => 120];
            $configuration['retries'] = 2;
        }

        $configuration['driver'] = $driver;
        $configuration['visibility'] = 'private';
        $configuration['throw'] = true;

        return $configuration;
    }

    public function localRoot(string $root): string
    {
        $root = $this->normalizeLocalRoot($root);
        $configuredBase = (string) config('filebeam.filesystems.local_root');
        File::ensureDirectoryExists($configuredBase);
        $base = realpath($configuredBase);
        if ($base === false) {
            throw new InvalidArgumentException('FILEBEAM_FILESTORE_ROOT must exist.');
        }

        $path = $base.DIRECTORY_SEPARATOR.$root;
        $existingPath = $path;
        while (! file_exists($existingPath) && ! is_link($existingPath)) {
            $parent = dirname($existingPath);
            if ($parent === $existingPath) {
                throw new InvalidArgumentException('Local filestore roots must remain inside FILEBEAM_FILESTORE_ROOT.');
            }
            $existingPath = $parent;
        }

        $resolved = realpath($existingPath);
        if ($resolved === false || ($resolved !== $base && ! str_starts_with($resolved, $base.DIRECTORY_SEPARATOR))) {
            throw new InvalidArgumentException('Local filestore roots must remain inside FILEBEAM_FILESTORE_ROOT.');
        }

        return $path;
    }

    public function normalizeLocalRoot(string $root): string
    {
        if ($root === '' || str_contains($root, "\0") || str_starts_with($root, '/') || str_starts_with($root, '\\') || preg_match('/^[A-Za-z]:[\\\\\/]/', $root) === 1) {
            throw new InvalidArgumentException('Local filestore roots must be relative to FILEBEAM_FILESTORE_ROOT.');
        }

        $segments = preg_split('/[\\\\\/]/', $root);
        if (! is_array($segments) || array_any($segments, fn (string $segment): bool => $segment === '' || $segment === '.' || $segment === '..')) {
            throw new InvalidArgumentException('Local filestore roots must not traverse outside FILEBEAM_FILESTORE_ROOT.');
        }

        return implode('/', $segments);
    }

    /**
     * @param  array<int, int>|null  $requested
     * @return array<int, int>
     */
    public function selectForPlan(Plan $plan, ?array $requested): array
    {
        $options = $this->optionsForPlan($plan);
        $allowedIds = array_column($options, 'id');

        if ($requested === null) {
            $requested = array_column(array_filter($options, fn (array $option): bool => $option['is_default']), 'id');
        }

        $selectedIds = array_map('intval', $requested)
                |> array_unique(...)
                |> array_values(...);

        if ($selectedIds === [] || array_diff($selectedIds, $allowedIds) !== []) {
            throw ValidationException::withMessages([
                'filestore_ids' => 'Storage is not currently available for this plan. Please contact the administrator.',
            ]);
        }

        return $selectedIds;
    }

    /** @return array<int, array{id: int, name: string, is_default: bool}> */
    public function optionsForPlan(Plan $plan): array
    {
        return $this->assignedTo($plan)
            ->get()
            ->filter(fn (Filestore $store): bool => $this->placementEnabled($store))
            ->map(fn (Filestore $store): array => [
                'id' => $store->id,
                'name' => $store->name,
                'is_default' => (bool) $store->getAttribute('is_default'),
            ])
            ->values()
            ->all();
    }

    /** @return Builder<Filestore> */
    private function assignedTo(Plan $plan): Builder
    {
        return Filestore::query()
            ->select('filestores.*', 'plan_filestore.is_default')
            ->join('plan_filestore', 'plan_filestore.filestore_id', '=', 'filestores.id')
            ->where('plan_filestore.plan_id', $plan->getKey())
            ->orderByDesc('plan_filestore.is_default')
            ->orderBy('filestores.id');
    }

    public function placementEnabled(Filestore $store): bool
    {
        if (! $store->placement_enabled) {
            return false;
        }

        $environmentDisks = $this->environmentDisks();

        if ($environmentDisks === null) {
            return $store->source !== 'environment' || is_string($store->disk_name);
        }

        return $store->source === 'environment'
            && is_string($store->disk_name)
            && in_array($store->disk_name, $environmentDisks, true);
    }
}
