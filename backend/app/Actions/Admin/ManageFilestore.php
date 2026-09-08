<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Models\AdminAudit;
use App\Models\Filestore;
use App\Models\Transfer;
use App\Models\User;
use App\Support\FilestoreRegistry;
use Illuminate\Support\Arr;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Gate;
use Illuminate\Support\Facades\Validator;
use Illuminate\Validation\ValidationException;
use InvalidArgumentException;
use Throwable;

class ManageFilestore
{
    /** @var array<string> */
    private const array Secrets = ['key', 'secret'];

    /**
     * @param  array<string, mixed>  $attributes
     *
     * @throws Throwable
     */
    public function create(User $actor, array $attributes): Filestore
    {
        Gate::forUser($actor)->authorize('create', Filestore::class);
        $this->ensureNotEnvironmentManaged();
        $validated = $this->validate($attributes, false);

        return DB::transaction(function () use ($actor, $validated): Filestore {
            $actor = User::query()->lockForUpdate()->findOrFail($actor->id);
            Gate::forUser($actor)->authorize('create', Filestore::class);
            $this->ensureNotEnvironmentManaged();

            $filestore = Filestore::query()->create($validated);
            $this->audit($actor, 'filestore.created', $filestore, ['created' => $this->auditAttributes($validated)]);

            return $filestore;
        });
    }

    private function ensureNotEnvironmentManaged(): void
    {
        if (app(FilestoreRegistry::class)->environmentManaged()) {
            throw ValidationException::withMessages(['filestore' => 'Filestores are controlled by the environment and cannot be changed in Admin.']);
        }
    }

    /**
     * @param  array<string, mixed>  $attributes
     * @return array<string, mixed>
     */
    private function validate(array $attributes, bool $updating, ?Filestore $current = null): array
    {
        $allowed = $updating ? ['name', 'placement_enabled', 'configuration'] : ['name', 'source', 'disk_name', 'driver', 'placement_enabled', 'configuration'];
        if (array_diff(array_keys($attributes), $allowed) !== []) {
            throw ValidationException::withMessages(['attributes' => 'Only supported filestore settings may be changed.']);
        }

        $rules = [
            'name' => ['required', 'string', 'max:255'],
            'placement_enabled' => ['required', 'boolean'],
            'configuration' => ['nullable', 'array'],
        ];
        if (! $updating) {
            $rules += ['source' => ['required', 'in:database,laravel'], 'disk_name' => ['nullable', 'string', 'max:255', 'unique:filestores,disk_name'], 'driver' => ['nullable', 'in:local,s3']];
        }
        $validated = Validator::make($attributes, $rules)->validate();

        if ($updating) {
            $validated['source'] = $current->source;
            $validated['disk_name'] = $current->disk_name;
            $validated['driver'] = $current->driver;
        }
        if (in_array($validated['source'], ['laravel', 'environment'], true)) {
            if (! is_string($validated['disk_name'])) {
                throw ValidationException::withMessages(['disk_name' => 'Choose an installed Laravel filesystem disk.']);
            }
            try {
                app(FilestoreRegistry::class)->disk(new Filestore($validated));
            } catch (InvalidArgumentException) {
                throw ValidationException::withMessages(['disk_name' => 'Choose an installed private Laravel filesystem disk.']);
            }
            $validated['driver'] = null;
            $validated['configuration'] = [];
        } else {
            if (! in_array($validated['driver'], ['local', 's3'], true)) {
                throw ValidationException::withMessages(['driver' => 'Choose local or S3 storage.']);
            }
            $existingConfiguration = is_array($current?->configuration) ? $current->configuration : [];
            $submittedConfiguration = $validated['configuration'] ?? [];
            if ($updating && $this->identityWasSubmitted($validated['driver'], $submittedConfiguration, $existingConfiguration)) {
                throw ValidationException::withMessages(['configuration' => 'Store identity settings cannot be changed. Create a new store instead.']);
            }
            $validated['configuration'] = $this->configuration($validated['driver'], $submittedConfiguration, $existingConfiguration);
        }

        if ($updating && $this->identityChanged($current, $validated)) {
            throw ValidationException::withMessages(['configuration' => 'A populated store identity cannot be changed. Create a new store instead.']);
        }

        return Arr::only($validated, ['name', 'source', 'disk_name', 'driver', 'placement_enabled', 'configuration']);
    }

    /**
     * @param  array<string, mixed>  $submitted
     * @param  array<string, mixed>  $existing
     */
    private function identityWasSubmitted(?string $driver, array $submitted, array $existing): bool
    {
        $identity = $driver === 'local' ? ['root'] : ['bucket', 'endpoint', 'region'];

        return array_any($identity, fn ($field) => array_key_exists($field, $submitted) && $submitted[$field] !== ($existing[$field] ?? null));
    }

    /**
     * @param  array<string, mixed>  $configuration
     * @param  array<string, mixed>  $existing
     * @return array<string, mixed>
     */
    private function configuration(string $driver, array $configuration, array $existing): array
    {
        $allowed = $driver === 'local' ? ['root'] : ['key', 'secret', 'region', 'bucket', 'endpoint', 'use_path_style_endpoint'];
        if (array_diff(array_keys($configuration), $allowed) !== []) {
            throw ValidationException::withMessages(['configuration' => 'Unsupported storage configuration field.']);
        }
        $configuration = array_filter($configuration, fn (mixed $value): bool => $value !== null && $value !== '');
        $configuration = array_replace($existing, $configuration);
        foreach (self::Secrets as $secret) {
            if (! array_key_exists($secret, $configuration) && array_key_exists($secret, $existing)) {
                $configuration[$secret] = $existing[$secret];
            }
        }
        if ($driver === 'local') {
            $directory = $configuration['root'] ?? null;
            if (! is_string($directory)) {
                throw ValidationException::withMessages(['configuration.root' => 'Enter a safe directory relative to the configured local storage root.']);
            }
            try {
                $configuration['root'] = app(FilestoreRegistry::class)->normalizeLocalRoot($directory);
                app(FilestoreRegistry::class)->localRoot($configuration['root']);
            } catch (InvalidArgumentException) {
                throw ValidationException::withMessages(['configuration.root' => 'The directory must remain inside the configured local storage root.']);
            }
        } else {
            $required = ['key', 'secret', 'region', 'bucket'];
            foreach ($required as $field) {
                if (! isset($configuration[$field]) || ! is_string($configuration[$field])) {
                    throw ValidationException::withMessages(["configuration.{$field}" => 'This S3 setting is required.']);
                }
            }
            if (isset($configuration['endpoint']) && (! filter_var($configuration['endpoint'], FILTER_VALIDATE_URL) || parse_url($configuration['endpoint'], PHP_URL_SCHEME) !== 'https' || $this->privateEndpoint($configuration['endpoint']))) {
                throw ValidationException::withMessages(['configuration.endpoint' => 'Use a public HTTPS endpoint.']);
            }
            $configuration['use_path_style_endpoint'] = filter_var($configuration['use_path_style_endpoint'] ?? false, FILTER_VALIDATE_BOOLEAN);
        }

        return $configuration;
    }

    private function privateEndpoint(string $endpoint): bool
    {
        $host = parse_url($endpoint, PHP_URL_HOST);
        if (! is_string($host) || filter_var($host, FILTER_VALIDATE_IP) === false) {
            return false;
        }

        return ! filter_var($host, FILTER_VALIDATE_IP, FILTER_FLAG_NO_PRIV_RANGE | FILTER_FLAG_NO_RES_RANGE);
    }

    /** @param array<string, mixed> $validated */
    private function identityChanged(Filestore $filestore, array $validated): bool
    {
        if (! $this->hasReferences($filestore)) {
            return false;
        }
        $identity = $filestore->driver === 'local' ? ['root'] : ['bucket', 'endpoint', 'region'];
        if (array_any($identity, fn ($field) => ($filestore->configuration[$field] ?? null) !== ($validated['configuration'][$field] ?? null))) {
            return true;
        }

        return $filestore->disk_name !== $validated['disk_name'] || $filestore->driver !== $validated['driver'];
    }

    private function hasPendingTransferSelection(Filestore $filestore): bool
    {
        return Transfer::query()->pending()->whereJsonContains('filestore_ids', $filestore->getKey())->exists();
    }

    private function hasReferences(Filestore $filestore): bool
    {
        return $filestore->locations()->exists()
            || $filestore->uploadAttempts()->exists()
            || $this->hasPendingTransferSelection($filestore);
    }

    /** @param array<string, mixed> $changes */
    private function audit(User $actor, string $action, Filestore $filestore, array $changes): void
    {
        AdminAudit::query()->create(['actor_id' => $actor->id, 'action' => $action, 'target_type' => Filestore::class, 'target_id' => $filestore->getKey(), 'changes' => $changes]);
    }

    /**
     * @param  array<string, mixed>  $attributes
     * @return array<string, mixed>
     */
    private function auditAttributes(array $attributes): array
    {
        return collect($attributes)->map(fn (mixed $value, string $key): mixed => $this->auditValue($key, $value))->all();
    }

    private function auditValue(string $attribute, mixed $value): mixed
    {
        if ($attribute !== 'configuration' || ! is_array($value)) {
            return $value;
        }

        return collect($value)->map(fn (mixed $configurationValue, string $key): mixed => in_array($key, self::Secrets, true) ? '[redacted]' : $configurationValue)->all();
    }

    /**
     * @param  array<string, mixed>  $attributes
     *
     * @throws Throwable
     */
    public function update(User $actor, Filestore $filestore, array $attributes): Filestore
    {
        Gate::forUser($actor)->authorize('update', $filestore);
        $this->ensureNotEnvironmentManaged();

        return DB::transaction(function () use ($actor, $filestore, $attributes): Filestore {
            $actor = User::query()->lockForUpdate()->findOrFail($actor->id);
            $filestore = Filestore::query()->lockForUpdate()->findOrFail($filestore->id);
            Gate::forUser($actor)->authorize('update', $filestore);
            $this->ensureNotEnvironmentManaged();

            $validated = $this->validate($attributes, true, $filestore);
            $changes = [];
            foreach ($validated as $attribute => $value) {
                if ($filestore->getAttribute($attribute) != $value) {
                    $changes[$attribute] = ['from' => $this->auditValue($attribute, $filestore->getAttribute($attribute)), 'to' => $this->auditValue($attribute, $value)];
                }
            }

            $filestore->forceFill($validated)->save();
            $this->audit($actor, 'filestore.updated', $filestore, $changes);

            return $filestore;
        });
    }

    /**
     * @throws Throwable
     */
    public function delete(User $actor, Filestore $filestore): void
    {
        Gate::forUser($actor)->authorize('delete', $filestore);
        $this->ensureNotEnvironmentManaged();

        DB::transaction(function () use ($actor, $filestore): void {
            $actor = User::query()->lockForUpdate()->findOrFail($actor->id);
            $filestore = Filestore::query()->lockForUpdate()->findOrFail($filestore->id);
            Gate::forUser($actor)->authorize('delete', $filestore);
            $this->ensureNotEnvironmentManaged();

            if ($this->hasReferences($filestore)) {
                throw ValidationException::withMessages(['filestore' => 'This store is referenced by committed data, upload attempts, or a pending transfer.']);
            }

            $id = $filestore->getKey();
            $filestore->delete();
            $this->audit($actor, 'filestore.deleted', $filestore, ['id' => ['from' => $id, 'to' => null]]);
        });
    }
}
