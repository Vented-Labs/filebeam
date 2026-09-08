<?php

declare(strict_types=1);

namespace App\Support\Installation;

use App\Enums\UserRole;
use App\Models\AdminAudit;
use App\Models\Filestore;
use App\Models\InstanceSetting;
use App\Models\Plan;
use App\Models\User;
use Database\Seeders\PlanSeeder;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Artisan;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;
use Illuminate\Validation\ValidationException;
use JsonException;
use Throwable;

readonly class CompleteInstallation
{
    public function __construct(
        private InstallationConfiguration $configuration,
        private EnvironmentWriter $environmentWriter,
        private InstallationState $state,
        private OptimizeInstallation $optimizer,
        private CacheConfiguration $cache,
    ) {}

    /** @param array<string, mixed> $validated
     * @throws JsonException
     * @throws Throwable
     */
    public function handle(array $validated): void
    {
        $this->state->locked(function () use ($validated): void {
            $state = $this->state->read();
            if ($state === null || ! in_array($state['status'] ?? null, ['pending', 'installing'], true) || ! is_string($state['id'] ?? null)) {
                throw ValidationException::withMessages(['installation' => 'This installation is no longer pending.']);
            }
            $installationId = $state['id'];
            $fingerprint = $this->fingerprint($validated);
            if (isset($state['configuration_fingerprint']) && $state['configuration_fingerprint'] !== $fingerprint) {
                throw ValidationException::withMessages(['installation' => 'Installation settings cannot be changed after the database has been claimed.']);
            }
            $connection = 'installation';
            config()->set("database.connections.{$connection}", $this->configuration->databaseConfiguration($validated['database']));
            DB::purge($connection);

            $claimed = Schema::connection($connection)->hasTable('installation_records');
            $record = $claimed ? DB::connection($connection)->table('installation_records')->first() : null;
            if ($record !== null && $record->installation_id !== $installationId) {
                throw ValidationException::withMessages(['database' => 'The selected database belongs to another installation.']);
            }
            if ($record !== null && $record->completed_at !== null) {
                $this->optimizer->handle();
                $this->state->write(array_replace($state, ['status' => 'completed', 'checkpoint' => 'completed', 'token_hash' => null]));
                $this->environmentWriter->write(['FILEBEAM_INSTALL_TOKEN' => null]);
                $this->state->writeGeneration();

                return;
            }
            foreach ($validated['storage'] as $index => $storage) {
                $this->configuration->testStorage($storage, "storage.{$index}");
            }
            $this->cache->test($validated['cache'] ?? ['driver' => 'file']);
            $createdClaimTable = false;
            if (! $claimed) {
                $this->configuration->testDatabase($validated['database']);
                $this->state->write(array_replace($state, ['status' => 'installing', 'checkpoint' => 'claiming', 'configuration_fingerprint' => $fingerprint]));
                Schema::connection($connection)->create('installation_records', function (Blueprint $table): void {
                    $table->id();
                    $table->uuid('installation_id')->unique();
                    $table->timestamp('completed_at')->nullable();
                    $table->timestamps();
                });
                $createdClaimTable = true;
            }
            if ($record === null) {
                if (! $createdClaimTable) {
                    $tables = Schema::connection($connection)->getTables();
                    if (($state['checkpoint'] ?? null) !== 'claiming' || ($state['configuration_fingerprint'] ?? null) !== $fingerprint || count($tables) !== 1 || $tables[0]['name'] !== 'installation_records') {
                        throw ValidationException::withMessages(['database' => 'The selected database has an unowned installation claim.']);
                    }
                }
                DB::connection($connection)->table('installation_records')->insert(['installation_id' => $installationId, 'created_at' => now(), 'updated_at' => now()]);
            }
            $this->state->write(array_replace($state, ['status' => 'installing', 'checkpoint' => 'claimed', 'configuration_fingerprint' => $fingerprint]));
            $this->configuration->probeSchemaPermissions(DB::connection($connection), 'The claimed database schema permissions could not be verified.');
            $this->applyRuntimeConfiguration($validated, $connection);
            $this->environmentWriter->write($this->environmentValues($validated));
            if (Artisan::call('migrate', ['--database' => $connection, '--force' => true]) !== 0) {
                throw ValidationException::withMessages(['database' => 'Database migrations could not be completed.']);
            }
            $this->state->write(array_replace($state, ['status' => 'installing', 'checkpoint' => 'migrated', 'configuration_fingerprint' => $fingerprint]));

            DB::connection($connection)->transaction(function () use ($validated, $installationId): void {
                app(PlanSeeder::class)->run();
                $plan = Plan::query()->default('default')->sole();
                $plan->forceFill(['placement_mode' => $validated['placement_mode']])->save();
                $admin = User::query()->where('email', $validated['admin']['email'])->first();
                if ($admin === null) {
                    $admin = new User;
                    $admin->forceFill(['name' => $validated['admin']['name'], 'username' => $validated['admin']['username'], 'normalized_username' => $validated['admin']['username'], 'email' => $validated['admin']['email'], 'password' => $validated['admin']['password'], 'email_verified_at' => now(), 'plan_id' => $plan->id, 'role' => UserRole::Admin])->save();
                }
                foreach ($validated['storage'] as $storage) {
                    $store = Filestore::query()->firstOrCreate(['name' => $storage['name']], ['source' => 'database', 'driver' => $storage['driver'], 'configuration' => $this->configuration->storageConfiguration($storage), 'placement_enabled' => true]);
                    $plan->filestores()->syncWithoutDetaching([$store->id => ['is_default' => true]]);
                }
                $enabled = $validated['instance']['visibility'] === 'public';
                foreach (['registration', 'anonymous_uploads'] as $key) {
                    InstanceSetting::query()->updateOrCreate(['key' => $key], ['value' => $enabled]);
                }
                AdminAudit::query()->firstOrCreate(['actor_id' => $admin->id, 'action' => 'installation.completed', 'target_type' => User::class, 'target_id' => (string) $admin->id], ['changes' => ['installation_id' => $installationId]]);
            });
            DB::table('installation_records')->where('installation_id', $installationId)->update(['completed_at' => now(), 'updated_at' => now()]);
            $this->optimizer->handle();
            $this->state->write(array_replace($state, ['status' => 'completed', 'checkpoint' => 'completed', 'configuration_fingerprint' => $fingerprint, 'token_hash' => null]));
            $this->environmentWriter->write(['FILEBEAM_INSTALL_TOKEN' => null]);
            $this->state->writeGeneration();
        });
    }

    /** @param array<string, mixed> $validated
     * @throws JsonException
     */
    private function fingerprint(array $validated): string
    {
        $configuration = $validated;
        $configuration['database']['password'] = hash('sha256', (string) ($configuration['database']['password'] ?? ''));
        $configuration['admin']['password'] = hash('sha256', $configuration['admin']['password']);
        if (isset($configuration['cache']['password'])) {
            $configuration['cache']['password'] = hash('sha256', $configuration['cache']['password']);
        }
        unset($configuration['admin']['password_confirmation'], $configuration['chunk_warning_acknowledged']);
        foreach ($configuration['storage'] as &$storage) {
            $storage['key'] = hash('sha256', (string) ($storage['key'] ?? ''));
            $storage['secret'] = hash('sha256', (string) ($storage['secret'] ?? ''));
        }
        unset($storage);

        return hash('sha256', json_encode($configuration, JSON_THROW_ON_ERROR));
    }

    /** @param array<string, mixed> $validated */
    private function applyRuntimeConfiguration(array $validated, string $connection): void
    {
        config()->set('database.default', $connection);
        config()->set('app.name', $validated['instance']['name']);
        config()->set('app.url', $validated['instance']['url']);
        config()->set('filebeam.branding.name', $validated['instance']['name']);
        config()->set('filebeam.username_domain', $validated['instance']['username_domain']);
        config()->set('filebeam.filesystems.environment', null);
    }

    /**
     * @param  array<string, mixed>  $validated
     * @return array<string, string|null>
     */
    private function environmentValues(array $validated): array
    {
        $database = $validated['database'];
        $cache = $this->cache->environmentValues($validated['cache'] ?? ['driver' => 'file']);
        if (config('installation.container') && config('installation.container_variant') === 'omnibus') {
            // The supervisor supplies this from the private valkey-password file at every boot.
            $cache['REDIS_PASSWORD'] = null;
            // Keep queue jobs isolated from cache flushes and cache locks.
            $cache['REDIS_DB'] = '0';
            $cache['REDIS_CACHE_DB'] = '1';
        }

        return [
            ...$cache,
            'APP_NAME' => $validated['instance']['name'],
            'FILEBEAM_NAME' => $validated['instance']['name'],
            'APP_URL' => $validated['instance']['url'],
            'FILEBEAM_USERNAME_DOMAIN' => $validated['instance']['username_domain'],
            'FILEBEAM_AUTO_UPDATES_ENABLED' => ($validated['instance']['auto_updates_enabled'] ?? false) ? 'true' : 'false',
            'CHUNK_MAX_SIZE' => (string) $validated['chunk_max_size'],
            'DB_CONNECTION' => $database['driver'],
            'DB_HOST' => $database['host'] ?? null,
            'DB_PORT' => isset($database['port']) ? (string) $database['port'] : null,
            'DB_SOCKET' => in_array($database['driver'], ['mysql', 'mariadb'], true) ? ($database['socket'] ?: null) : null,
            'DB_DATABASE' => $database['database'],
            'DB_USERNAME' => $database['username'] ?? null,
            'DB_PASSWORD' => $database['password'] ?? null,
            'DB_SSLMODE' => $database['sslmode'] ?? null,
            'DB_URL' => null,
            'QUEUE_CONNECTION' => config('installation.container') && config('installation.container_variant') === 'omnibus' ? 'redis' : 'database',
            'FILEBEAM_CRON_QUEUE_ENABLED' => config('installation.container') ? 'false' : null,
            'FILEBEAM_FILESYSTEMS' => null,
            'FILEBEAM_REGISTRATION_ENABLED' => null,
            'FILEBEAM_ANONYMOUS_UPLOADS_ENABLED' => null,
            'FILEBEAM_USERNAME_ROUTING_ENABLED' => null,
        ];
    }
}
