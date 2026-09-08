<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Filament\Pages\Updates;
use App\Models\User;
use App\Services\ReleaseChecker;
use Filament\Facades\Filament;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\Http;
use Illuminate\Support\Facades\Process;
use Livewire\Livewire;

beforeEach(function (): void {
    Filament::setCurrentPanel(Filament::getPanel('admin'));
    config()->set('filebeam.updates.state_path', storage_path('framework/testing/updates-'.bin2hex(random_bytes(4))));
    $this->defaultDatabaseConnection = config('database.default');
    $this->defaultDatabaseDriver = config('database.connections.'.$this->defaultDatabaseConnection.'.driver');
});

afterEach(function (): void {
    config()->set('database.connections.'.$this->defaultDatabaseConnection.'.driver', $this->defaultDatabaseDriver);
    File::deleteDirectory(config('filebeam.updates.state_path'));
});

test('release checks verify and persist a signed current catalog', function () {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('version', [
        ...config('version'),
        'version' => '0.1.0',
        'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair)),
    ]);
    Http::fake(['https://releases.filebeam.io/index.json' => Http::response(signedCatalog($keypair, [release('v0.2.0')]))]);

    $state = app(ReleaseChecker::class)->check();

    expect($state['state'])->toBe('available')
        ->and($state['latest']['tag'])->toBe('v0.2.0')
        ->and($state['latest']['upgradeable'])->toBeTrue()
        ->and($state['error'])->toBeNull();
});

test('failed checks preserve the last known release as stale state', function () {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('version', [...config('version'), 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    Http::fakeSequence()
        ->push(signedCatalog($keypair, [release('v0.2.0')]))
        ->push([], 503);
    app(ReleaseChecker::class)->check();

    $state = app(ReleaseChecker::class)->check();

    expect($state['state'])->toBe('error')
        ->and($state['latest']['tag'])->toBe('v0.2.0')
        ->and($state['error'])->toContain('503');
});

test('release checks reject expired and unsigned catalogs', function () {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('version', [...config('version'), 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    Http::fakeSequence()
        ->push(signedCatalog($keypair, [release('v0.2.0')], now('UTC')->subMinute()->toIso8601String()))
        ->push(['signed' => base64_encode('{}'), 'signature' => base64_encode('invalid')]);

    expect(app(ReleaseChecker::class)->check()['error'])->toBe('Release catalog is expired.');

    expect(app(ReleaseChecker::class)->check()['error'])->toBe('Release catalog signature verification failed.');
});

test('release checks mark releases requiring an intermediary as unavailable to upgrade', function () {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('version', [...config('version'), 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    Http::fake(['https://releases.filebeam.io/index.json' => Http::response(signedCatalog($keypair, [release('v2.0.0', ['minimum_version' => '1.0.0'])]))]);

    $state = app(ReleaseChecker::class)->check();

    expect($state['latest']['upgradeable'])->toBeFalse()
        ->and($state['latest']['warning'])->toContain('intermediate');
});

test('automatic updates install only a checked compatible release when enabled', function () {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('filebeam.updates.auto_enabled', true);
    config()->set('version', [...config('version'), 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    Http::preventStrayRequests();
    Http::fake(['https://releases.filebeam.io/index.json' => Http::response(signedCatalog($keypair, [release('v0.2.0')]))]);
    Process::preventStrayProcesses();
    Process::fake();
    $checker = new class extends ReleaseChecker
    {
        public function availability(bool $requireCron = true): array
        {
            return ['available' => true, 'reasons' => []];
        }
    };

    $state = $checker->checkAndInstallAutomaticUpdate();

    expect($state['latest']['tag'])->toBe('v0.2.0');
    Process::assertRan(fn ($process): bool => $process->path === dirname(base_path())
        && $process->timeout === 900
        && in_array('update.php', $process->command, true)
        && in_array('v0.2.0', $process->command, true));
});

test('PostgreSQL availability requires a successful backup preflight', function () {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('version', [...config('version'), 'distribution' => 'package', 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    config()->set('database.connections.'.config('database.default').'.driver', 'pgsql');
    Process::preventStrayProcesses();
    Process::fake(['*' => Process::result(exitCode: 1)]);

    $availability = app(ReleaseChecker::class)->availability(requireCron: false);

    if (is_file('/.dockerenv')) {
        expect($availability['available'])->toBeFalse()
            ->and($availability['reasons'])->toContain('Self-updates are unavailable in containers.');
        Process::assertNothingRan();

        return;
    }

    expect($availability['available'])->toBeFalse()
        ->and($availability['reasons'])->toContain('PostgreSQL updates require matching pg_dump/pg_restore and a reachable database. Run php update.php --check-backup.');
    Process::assertRan(fn ($process): bool => $process->path === dirname(base_path())
        && $process->timeout === 45
        && in_array('update.php', $process->command, true)
        && in_array('--check-backup', $process->command, true));
});

test('automatic PostgreSQL updates stop when the backup preflight fails', function () {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('filebeam.updates.auto_enabled', true);
    config()->set('version', [...config('version'), 'distribution' => 'package', 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    config()->set('database.connections.'.config('database.default').'.driver', 'pgsql');
    Http::preventStrayRequests();
    Http::fake(['https://releases.filebeam.io/index.json' => Http::response(signedCatalog($keypair, [release('v0.2.0')]))]);
    Process::preventStrayProcesses();
    Process::fake(['*' => Process::result(exitCode: 1)]);

    $state = app(ReleaseChecker::class)->checkAndInstallAutomaticUpdate();

    if (is_file('/.dockerenv')) {
        $availability = app(ReleaseChecker::class)->availability(requireCron: false);

        expect($state['state'])->toBe('available')
            ->and($availability['available'])->toBeFalse()
            ->and($availability['reasons'])->toContain('Self-updates are unavailable in containers.');
        Process::assertNothingRan();

        return;
    }

    expect($state['state'])->toBe('available');
    Process::assertRan(fn ($process): bool => in_array('--check-backup', $process->command, true));
    Process::assertDidntRun(fn ($process): bool => in_array('v0.2.0', $process->command, true));
});

test('automatic PostgreSQL updates run a successful backup preflight before the updater', function () {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('filebeam.updates.auto_enabled', true);
    config()->set('version', [...config('version'), 'distribution' => 'package', 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    config()->set('database.connections.'.config('database.default').'.driver', 'pgsql');
    Http::preventStrayRequests();
    Http::fake(['https://releases.filebeam.io/index.json' => Http::response(signedCatalog($keypair, [release('v0.2.0')]))]);
    Process::preventStrayProcesses();
    Process::fake();

    $state = app(ReleaseChecker::class)->checkAndInstallAutomaticUpdate();

    if (is_file('/.dockerenv')) {
        $availability = app(ReleaseChecker::class)->availability(requireCron: false);

        expect($state['state'])->toBe('available')
            ->and($availability['available'])->toBeFalse()
            ->and($availability['reasons'])->toContain('Self-updates are unavailable in containers.');
        Process::assertNothingRan();

        return;
    }

    expect($state['latest']['tag'])->toBe('v0.2.0');
    Process::assertRan(fn ($process): bool => $process->path === dirname(base_path())
        && $process->timeout === 45
        && in_array('--check-backup', $process->command, true));
    Process::assertRan(fn ($process): bool => $process->path === dirname(base_path())
        && $process->timeout === 900
        && in_array('v0.2.0', $process->command, true));
});

test('automatic updates do not invoke the updater when disabled', function () {
    config()->set('filebeam.updates.auto_enabled', false);
    $keypair = sodium_crypto_sign_keypair();
    config()->set('version', [...config('version'), 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    Http::preventStrayRequests();
    Http::fake(['https://releases.filebeam.io/index.json' => Http::response(signedCatalog($keypair, [release('v0.2.0')]))]);
    Process::preventStrayProcesses();
    Process::fake();

    $state = app(ReleaseChecker::class)->checkAndInstallAutomaticUpdate();

    expect($state['state'])->toBe('available');
    Process::assertNothingRan();
});

test('automatic updates skip unavailable and incompatible packages', function (string $scenario) {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('filebeam.updates.auto_enabled', true);
    config()->set('version', [...config('version'), 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    $releases = match ($scenario) {
        'current' => [],
        'incompatible' => [release('v2.0.0', ['minimum_version' => '1.0.0'])],
        default => [release('v0.2.0')],
    };
    Http::preventStrayRequests();
    Http::fake(['https://releases.filebeam.io/index.json' => Http::response($scenario === 'invalid signature' ? ['signed' => 'invalid', 'signature' => 'invalid'] : signedCatalog($keypair, $releases))]);
    Process::preventStrayProcesses();
    Process::fake();
    $checker = new class extends ReleaseChecker
    {
        public function availability(bool $requireCron = true): array
        {
            return ['available' => true, 'reasons' => []];
        }
    };

    $checker->checkAndInstallAutomaticUpdate();

    Process::assertNothingRan();
})->with(['current', 'incompatible', 'invalid signature']);

test('automatic updates skip source installations and unsupported database drivers', function (string $scenario) {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('filebeam.updates.auto_enabled', true);
    config()->set('version', [...config('version'), 'distribution' => $scenario === 'source' ? 'source' : 'package', 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    if ($scenario === 'unsupported driver') {
        config()->set('database.connections.'.config('database.default').'.driver', 'sqlsrv');
    }
    Http::preventStrayRequests();
    Http::fake(['https://releases.filebeam.io/index.json' => Http::response(signedCatalog($keypair, [release('v0.2.0')]))]);
    Process::preventStrayProcesses();
    Process::fake();

    $state = app(ReleaseChecker::class)->checkAndInstallAutomaticUpdate();

    expect($state['state'])->toBe('available');
    Process::assertNothingRan();
})->with(['source', 'unsupported driver']);

test('only admins can access updates and page reads do not fetch the network', function () {
    Http::preventStrayRequests();
    $user = User::factory()->create();
    $admin = User::factory()->create(['role' => UserRole::Admin]);

    $this->actingAs($user, 'admin');
    expect(Updates::canAccess())->toBeFalse();

    $this->actingAs($admin, 'admin');
    Livewire::test(Updates::class)
        ->assertOk()
        ->assertSee('Installed version');
});

test('source installations cannot queue self-updates', function () {
    expect(app(ReleaseChecker::class)->availability()['available'])->toBeFalse()
        ->and(app(ReleaseChecker::class)->availability()['reasons'])->toContain('Self-updates are available only for package installations.');
});

test('upgrade queue is atomically written only when the updater is available', function () {
    $keypair = sodium_crypto_sign_keypair();
    config()->set('version', [...config('version'), 'distribution' => 'package', 'update_public_key' => base64_encode(sodium_crypto_sign_publickey($keypair))]);
    Http::fake(['https://releases.filebeam.io/index.json' => Http::response(signedCatalog($keypair, [release('v0.2.0')]))]);
    $checker = new class extends ReleaseChecker
    {
        public function availability(bool $requireCron = true): array
        {
            return ['available' => true, 'reasons' => []];
        }
    };
    $checker->check();
    $checker->queue('v0.2.0', 'admin@example.test');

    expect(json_decode(File::get(config('filebeam.updates.state_path').'/pending.json'), true))
        ->toMatchArray(['tag' => 'v0.2.0', 'requested_by' => 'admin@example.test'])
        ->and(File::exists(config('filebeam.updates.state_path').'/pending.json.tmp'))->toBeFalse();
});

test('stale release-check state cannot queue an upgrade', function () {
    $checker = new class extends ReleaseChecker
    {
        public function availability(bool $requireCron = true): array
        {
            return ['available' => true, 'reasons' => []];
        }
    };

    $statePath = config('filebeam.updates.state_path');
    File::ensureDirectoryExists($statePath);
    File::put($statePath.'/release-check.json', json_encode([
        'state' => 'error',
        'latest' => ['tag' => 'v0.2.0', 'upgradeable' => true],
    ], JSON_THROW_ON_ERROR));

    expect(fn (): mixed => $checker->queue('v0.2.0', 'admin@example.test'))
        ->toThrow(RuntimeException::class, 'Run a successful release check');
});

/** @param array<string, mixed> $overrides
 * @return array<string, mixed>
 */
function release(string $tag, array $overrides = []): array
{
    return [...[
        'tag' => $tag,
        'version' => ltrim($tag, 'v'),
        'package' => [],
    ], ...$overrides];
}

/** @param list<array<string, mixed>> $releases
 * @return array<string, string>
 */
function signedCatalog(string $keypair, array $releases, ?string $expiresAt = null): array
{
    $payload = json_encode([
        'schema' => 1,
        'generation' => 1,
        'published_at' => now('UTC')->subMinute()->toIso8601String(),
        'expires_at' => $expiresAt ?? now('UTC')->addDay()->toIso8601String(),
        'releases' => $releases,
    ], JSON_THROW_ON_ERROR);

    return [
        'signed' => base64_encode($payload),
        'signature' => base64_encode(sodium_crypto_sign_detached($payload, sodium_crypto_sign_secretkey($keypair))),
    ];
}
