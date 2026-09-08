<?php

declare(strict_types=1);

use App\Support\FilestoreRegistry;
use App\Support\Installation\InstallationConfiguration;
use Illuminate\Support\Facades\File;
use Illuminate\Validation\ValidationException;

use function Pest\Laravel\mock;

beforeEach(function (): void {
    $this->configurationDirectory = storage_path('framework/testing/configuration-'.bin2hex(random_bytes(8)));
    File::ensureDirectoryExists($this->configurationDirectory);
    config()->set('installation.sqlite_directory', $this->configurationDirectory);
    config()->set('filebeam.filesystems.local_root', $this->configurationDirectory.'/files');
    app()->instance('installation.external_environment', []);
});

afterEach(function (): void {
    File::deleteDirectory($this->configurationDirectory);
});

test('rejects local storage roots that escape the private filestore root', function (): void {
    $input = installationInput();
    $input['storage'][0]['root'] = '../public';

    try {
        app(InstallationConfiguration::class)->validate($input);
        test()->fail('Expected storage validation to fail.');
    } catch (ValidationException $exception) {
        expect($exception->errors())->toHaveKey('storage.0.root');
    }
});

test('container local storage requires an existing writable filestore mount', function (): void {
    $mount = $this->configurationDirectory.'/missing-storage-mount';
    $container = config('installation.container');
    $localRoot = config('filebeam.filesystems.local_root');
    config()->set(['installation.container' => true, 'filebeam.filesystems.local_root' => $mount]);

    try {
        app(InstallationConfiguration::class)->validate(installationInput());
        test()->fail('Expected missing container filestore mount to fail validation.');
    } catch (ValidationException $exception) {
        expect($exception->errors())->toHaveKey('storage.0.root');
    } finally {
        config()->set(['installation.container' => $container, 'filebeam.filesystems.local_root' => $localRoot]);
    }
});

test('rejects unsafe instance URLs', function (string $url): void {
    $input = installationInput();
    $input['instance']['url'] = $url;

    try {
        app(InstallationConfiguration::class)->validate($input);
        test()->fail('Expected URL validation to fail.');
    } catch (ValidationException $exception) {
        expect($exception->errors())->toHaveKey('instance.url');
    }
})->with(['http://example.com', 'http://127.attacker.example', 'https://example.com/path', 'https://user:pass@example.com/', 'https://example.com/?query=yes', 'https://example.com/#fragment']);

test('accepts valid username domains', function (string $domain): void {
    $input = installationInput();
    $input['instance']['username_domain'] = $domain;

    expect(app(InstallationConfiguration::class)->validate($input)['instance']['username_domain'])->toBe($domain);
})->with(['example.com', 'users.example.com']);

test('rejects username domains with schemes or paths', function (string $domain): void {
    $input = installationInput();
    $input['instance']['username_domain'] = $domain;

    try {
        app(InstallationConfiguration::class)->validate($input);
        test()->fail('Expected username domain validation to fail.');
    } catch (ValidationException $exception) {
        expect($exception->errors())
            ->toHaveKey('instance.username_domain')
            ->not->toHaveKey('instance.url');
        expect($exception->errors()['instance.username_domain'])->toContain('Username domain must be a hostname without a scheme or path.');
    }
})->with(['https://users.example.com', 'users.example.com/path']);

test('rejects invalid public URLs separately from username domains', function (): void {
    $input = installationInput();
    $input['instance']['url'] = 'https://example.com/path';
    $input['instance']['username_domain'] = 'users.example.com';

    try {
        app(InstallationConfiguration::class)->validate($input);
        test()->fail('Expected public URL validation to fail.');
    } catch (ValidationException $exception) {
        expect($exception->errors())
            ->toHaveKey('instance.url')
            ->not->toHaveKey('instance.username_domain');
    }
});

test('rejects invalid direct database and storage probe candidates', function (): void {
    expect(fn (): mixed => app(InstallationConfiguration::class)->testDatabase(['driver' => 'sqlsrv']))->toThrow(ValidationException::class);
    try {
        app(InstallationConfiguration::class)->testStorage(['driver' => 'invalid']);
        test()->fail('Expected storage validation to fail.');
    } catch (ValidationException $exception) {
        expect($exception->errors())->toHaveKey('storage.driver');
    }
});

test('attributes direct storage validation errors to the supplied prefix', function (): void {
    try {
        app(InstallationConfiguration::class)->testStorage(['driver' => 'invalid'], 'storage.1');
        test()->fail('Expected storage validation to fail.');
    } catch (ValidationException $exception) {
        expect($exception->errors())->toHaveKey('storage.1.driver');
    }
});

test('attributes storage probe failures to the supplied prefix', function (): void {
    $registry = mock(FilestoreRegistry::class);
    $registry->shouldReceive('normalizeLocalRoot')->times(4)->andReturn('primary');
    $registry->shouldReceive('localRoot')->times(4)->andReturn($this->configurationDirectory.'/files/primary');
    $registry->shouldReceive('disk')->twice()->andThrow(new RuntimeException('Probe failed.'));
    $configuration = app(InstallationConfiguration::class);

    foreach ([['storage.1'], []] as $arguments) {
        $attribute = $arguments[0] ?? 'storage';
        try {
            $configuration->testStorage(['driver' => 'local', 'root' => 'primary'], ...$arguments);
            test()->fail('Expected storage probe to fail.');
        } catch (ValidationException $exception) {
            expect($exception->errors())->toHaveKey($attribute);
        }
    }
});

test('attaches password confirmation failures to the confirmation field', function (string $confirmation): void {
    $input = installationInput();
    $input['admin']['password_confirmation'] = $confirmation;

    try {
        app(InstallationConfiguration::class)->validate($input);
        test()->fail('Expected password confirmation validation to fail.');
    } catch (ValidationException $exception) {
        expect($exception->errors())
            ->toHaveKey('admin.password_confirmation')
            ->not->toHaveKey('admin.password');
    }
})->with(['different password', '']);

test('Unix socket candidates use the correct connector settings without a TCP host', function (string $driver, string $socket): void {
    $input = installationInput();
    $input['database'] = ['driver' => $driver, 'transport' => 'socket', 'socket' => $socket, 'database' => 'filebeam', 'username' => 'owner', 'password' => 'secret'];
    $validated = app(InstallationConfiguration::class)->validate($input);
    $connection = app(InstallationConfiguration::class)->databaseConfiguration($validated['database']);

    expect($connection['host'])->toBe($driver === 'pgsql' ? $socket : 'localhost');
    expect($connection['unix_socket'])->toBe($driver === 'pgsql' ? '' : $socket);
    expect($connection['port'])->toBe($driver === 'pgsql' ? 5432 : 3306);
})->with([
    ['mysql', '/run/mysqld/mysqld.sock'],
    ['mariadb', '/run/mysqld/mysqld.sock'],
    ['pgsql', '/var/run/postgresql'],
]);

test('TCP candidates do not inherit a configured Unix socket', function (): void {
    config()->set('database.connections.mysql.unix_socket', '/old/socket');
    $connection = app(InstallationConfiguration::class)->databaseConfiguration(['driver' => 'mysql', 'transport' => 'tcp', 'socket' => '/ignored/socket', 'host' => 'db.example.test', 'port' => 3307, 'database' => 'filebeam', 'username' => 'owner']);
    expect($connection['unix_socket'])->toBe('')->and($connection['host'])->toBe('db.example.test')->and($connection['port'])->toBe(3307);
});

test('socket candidates reject missing relative and unsafe paths before connecting', function (mixed $socket): void {
    try {
        app(InstallationConfiguration::class)->testDatabase(['driver' => 'mysql', 'transport' => 'socket', 'socket' => $socket, 'database' => 'filebeam', 'username' => 'owner']);
        test()->fail('Expected socket validation to fail.');
    } catch (ValidationException $exception) {
        expect($exception->errors())->toHaveKey('database.socket');
    }
})->with([null, '', 'relative.sock', '/tmp/socket;host=elsewhere', "/tmp/socket\n", [['/tmp/socket']]]);

test('deployment-controlled database sockets and automatic updates cannot be overwritten', function (string $key): void {
    app()->instance('installation.external_environment', [$key => 'deployment-controlled']);
    try {
        app(InstallationConfiguration::class)->validate(installationInput());
        test()->fail('Expected environment lock validation to fail.');
    } catch (ValidationException $exception) {
        expect($exception->errors())->toHaveKey($key);
    }
})->with(['DB_SOCKET', 'FILEBEAM_AUTO_UPDATES_ENABLED']);

test('deployment-provided username routing cannot be changed by the installer', function (): void {
    app()->instance('installation.external_environment', ['FILEBEAM_USERNAME_ROUTING_ENABLED' => 'false']);

    expect(fn (): array => app(InstallationConfiguration::class)->validate(installationInput()))
        ->toThrow(ValidationException::class);
});

test('rejects sqlite traversal and symlink escapes while creating an approved missing file', function (): void {
    $base = storage_path('framework/testing/installation-sqlite');
    $outside = storage_path('framework/testing/installation-outside');
    File::deleteDirectory($base);
    File::deleteDirectory($outside);
    File::ensureDirectoryExists($base);
    File::ensureDirectoryExists($outside);
    config()->set('installation.sqlite_directory', $base);
    $configuration = app(InstallationConfiguration::class);

    expect(fn (): mixed => $configuration->testDatabase(['driver' => 'sqlite', 'database' => $base.'/../installation-outside/escape.sqlite']))->toThrow(ValidationException::class);
    symlink($outside, $base.'/escape');
    expect(fn (): mixed => $configuration->testDatabase(['driver' => 'sqlite', 'database' => $base.'/escape/escape.sqlite']))->toThrow(ValidationException::class);
    expect(fn (): mixed => $configuration->testDatabase(['driver' => 'sqlite', 'database' => $base.'/approved.sqlite']))->not->toThrow(ValidationException::class);
    expect(File::exists($base.'/approved.sqlite'))->toBeTrue();
    File::deleteDirectory($base);
    File::deleteDirectory($outside);
});

/** @return array<string, mixed> */
function installationInput(): array
{
    return ['database' => ['driver' => 'sqlite', 'database' => config('installation.sqlite_directory').'/installation-test.sqlite'], 'instance' => ['name' => 'Filebeam', 'url' => 'http://localhost', 'username_domain' => null, 'visibility' => 'public'], 'storage' => [['name' => 'Private', 'driver' => 'local', 'root' => 'installation-test', 'use_path_style_endpoint' => false]], 'placement_mode' => 'distribute', 'admin' => ['name' => 'Installer', 'username' => 'installer', 'email' => 'installer@example.test', 'password' => 'correct horse battery staple', 'password_confirmation' => 'correct horse battery staple', 'email_ownership_confirmed' => true], 'chunk_max_size' => 17];
}
