<?php

declare(strict_types=1);

use App\Support\Installation\CacheConfiguration;
use App\Support\Installation\ChunkSize;
use App\Support\Installation\EnvironmentWriter;
use App\Support\Installation\InstallationState;
use App\Support\Installation\OptimizeInstallation;
use Dotenv\Dotenv;
use Illuminate\Contracts\Foundation\Application;
use Illuminate\Http\Middleware\TrustHosts;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Facades\URL;
use Illuminate\Support\Str;
use Inertia\Testing\AssertableInertia as Assert;
use Symfony\Component\HttpFoundation\Exception\SuspiciousOperationException;

beforeEach(function (): void {
    $this->originalDatabase = config('database.default');
    app()->instance('installation.external_environment', []);
    $this->installationDirectory = storage_path('framework/testing/installer-'.Str::uuid());
    File::ensureDirectoryExists($this->installationDirectory.'/database');
    config()->set([
        'app.url' => 'http://localhost',
        'installation.state_directory' => $this->installationDirectory.'/state',
        'installation.environment_path' => $this->installationDirectory.'/.env',
        'installation.sqlite_directory' => $this->installationDirectory.'/database',
        'filebeam.filesystems.local_root' => $this->installationDirectory.'/files',
        'cache.stores.file.path' => $this->installationDirectory.'/cache/data',
        'cache.stores.file.lock_path' => $this->installationDirectory.'/cache/locks',
    ]);
    $this->withoutVite();
    URL::forceRootUrl('http://localhost');
    $this->withHeader('Origin', 'http://localhost');
});

afterEach(function (): void {
    config()->set('database.default', $this->originalDatabase);
    DB::setDefaultConnection($this->originalDatabase);
    DB::purge('installation');
    File::deleteDirectory($this->installationDirectory);
});

test('first boot renders without a key or database and generates a server-only token', function (): void {
    config()->set('app.key', null);
    config()->set('database.default', 'unconfigured');
    config()->set('cache.default', 'database');
    $page = $this->get('/install')->assertOk()->assertInertia(fn (Assert $page) => $page
        ->component('Install')->where('bootstrapRequired', true)->has('challenge')->missing('token'));
    $challenge = $page->viewData('page')['props']['challenge'];
    $response = $this->withHeader('X-Installation-Challenge', $challenge)->postJson('/install/bootstrap')->assertOk();
    $environment = Dotenv::createArrayBacked($this->installationDirectory)->load();
    expect($environment['FILEBEAM_INSTALL_TOKEN'])->toHaveLength(64);
    expect($environment)->toMatchArray(['APP_ENV' => 'production', 'APP_DEBUG' => 'false', 'SESSION_DRIVER' => 'cookie', 'CACHE_STORE' => 'file']);
    expect($response->getContent())->not->toContain($environment['FILEBEAM_INSTALL_TOKEN'], $environment['APP_KEY']);
    expect(fileperms($this->installationDirectory.'/.env') & 0777)->toBe(0600);

    $this->postJson('/install/bootstrap')->assertOk();
    expect(Dotenv::createArrayBacked($this->installationDirectory)->load())->toBe($environment);
    $this->get('/install')->assertOk()->assertInertia(fn (Assert $page) => $page->where('bootstrapRequired', false));
    $this->postJson('/install/configuration')->assertUnauthorized();
    $this->withHeader('X-Installation-Token', $environment['FILEBEAM_INSTALL_TOKEN'])
        ->postJson('/install/configuration')->assertOk()->assertJsonPath('defaults.instance.name', 'Filebeam')->assertJsonPath('chunks.maximum', 25_000_000);
});

test('light container bootstraps and loads durable defaults across installer requests', function (): void {
    config()->set(['installation.container' => true, 'installation.container_variant' => 'light']);
    app()->instance('installation.external_environment', ['FILEBEAM_CONTAINER' => 'true', 'FILEBEAM_VARIANT' => 'light']);

    $page = $this->get('/install')->assertOk();
    $challenge = $page->viewData('page')['props']['challenge'];
    $this->withHeader('X-Installation-Challenge', $challenge)->postJson('/install/bootstrap')->assertOk();
    $environment = Dotenv::createArrayBacked($this->installationDirectory)->load();

    $this->withHeader('X-Installation-Token', $environment['FILEBEAM_INSTALL_TOKEN'])
        ->postJson('/install/configuration')
        ->assertOk()
        ->assertJsonPath('defaults.database.driver', 'sqlite')
        ->assertJsonPath('defaults.database.database', $this->installationDirectory.'/database/database.sqlite')
        ->assertJsonPath('defaults.cache.driver', 'file');

    expect($environment['FILEBEAM_CRON_QUEUE_ENABLED'])->toBe('false');
});

test('bootstrap rejects cross-origin and missing challenges without writing configuration', function (): void {
    $this->postJson('/install/bootstrap')->assertStatus(419);
    $this->withHeader('Origin', 'https://attacker.example')->postJson('/install/bootstrap')->assertForbidden();
    expect(file_exists($this->installationDirectory.'/.env'))->toBeFalse();
});

test('production setup accepts its unconfigured hostname and restores host restrictions after installation', function (): void {
    $environment = app('env');
    app()->instance('env', 'production');

    try {
        $this->get('http://setup.example.test/install')->assertForbidden();
        $page = $this->get('https://setup.example.test/install')->assertOk();
        $challenge = $page->viewData('page')['props']['challenge'];

        $this->withHeader('Origin', 'https://attacker.example.test')
            ->withHeader('X-Installation-Challenge', $challenge)
            ->postJson('https://setup.example.test/install/bootstrap')->assertForbidden();
        $this->withHeader('Origin', 'https://setup.example.test')
            ->postJson('https://setup.example.test/install/bootstrap')->assertOk();
        $values = Dotenv::createArrayBacked($this->installationDirectory)->load();
        $this->withHeader('X-Installation-Token', $values['FILEBEAM_INSTALL_TOKEN'])
            ->postJson('https://setup.example.test/install/configuration')->assertOk();

        app(InstallationState::class)->write(['id' => (string) Str::uuid(), 'status' => 'completed']);
        config()->set('app.url', 'https://setup.example.test');
        $this->get('https://setup.example.test/up')->assertOk();
        $this->get('https://attacker.example.test/up')->assertStatus(400);
    } finally {
        app()->instance('env', $environment);
        Request::setTrustedHosts([]);
    }
});

test('installed and inconsistent state keep every installer endpoint closed even without env or database', function (string $status): void {
    app(InstallationState::class)->write(['id' => (string) Str::uuid(), 'status' => $status]);
    config()->set('app.key', null);
    config()->set('database.default', 'unconfigured');
    $this->get('/install')->assertNotFound();
    foreach (['bootstrap', 'configuration', 'database', 'cache', 'storage', 'complete'] as $endpoint) {
        $this->postJson('/install/'.$endpoint)->assertNotFound();
    }
    $this->putJson('/install/probe')->assertNotFound();
})->with(['completed', 'invalid']);

test('a pre-existing environment or sqlite data cannot be claimed through the browser', function (): void {
    File::put($this->installationDirectory.'/.env', 'APP_NAME=Existing');
    $this->get('/install')->assertNotFound();
    File::delete($this->installationDirectory.'/.env');
    File::put($this->installationDirectory.'/database/database.sqlite', 'existing database');
    $this->get('/install')->assertNotFound();
});

test('pending installation gates application traffic without querying the database', function (): void {
    config()->set('database.default', 'unconfigured');
    $this->get('/')->assertRedirect('/install');
    $this->postJson('/api/v1/transfers')->assertServiceUnavailable();
});

test('authenticated storage checks verify real local read write and delete', function (): void {
    app(InstallationState::class)->write(['id' => (string) Str::uuid(), 'status' => 'pending', 'token_hash' => hash('sha256', 'setup-secret')]);
    $this->withHeader('X-Installation-Token', 'setup-secret')->postJson('/install/storage', [
        'storage' => ['driver' => 'local', 'root' => 'primary'],
    ])->assertOk();
    expect(File::allFiles($this->installationDirectory.'/files'))->toBeEmpty();
    $this->postJson('/install/storage', ['storage' => ['driver' => 'local', 'root' => '../escape']])->assertUnprocessable()->assertJsonValidationErrors('storage.root');
});

test('chunk probe streams raw bytes and is token protected with a finite budget', function (): void {
    app(InstallationState::class)->write(['id' => (string) Str::uuid(), 'status' => 'pending', 'token_hash' => hash('sha256', 'setup-secret')]);
    $server = ['CONTENT_TYPE' => 'application/octet-stream', 'CONTENT_LENGTH' => 32, 'HTTP_ORIGIN' => 'http://localhost', 'HTTP_ACCEPT' => 'application/json'];
    $this->call('PUT', '/install/probe', [], [], [], $server, str_repeat('a', 32))->assertUnauthorized();
    $server['HTTP_X_INSTALLATION_TOKEN'] = 'setup-secret';
    for ($attempt = 0; $attempt < 5; $attempt++) {
        $this->call('PUT', '/install/probe', [], [], [], $server, str_repeat('a', 32))->assertOk()->assertJsonPath('bytes', 32);
    }
    $this->call('PUT', '/install/probe', [], [], [], $server, str_repeat('a', 32))->assertStatus(429);
});

test('installer validation errors never echo submitted credentials', function (): void {
    app(InstallationState::class)->write(['id' => (string) Str::uuid(), 'status' => 'pending', 'token_hash' => hash('sha256', 'setup-secret')]);
    $response = $this->withHeader('X-Installation-Token', 'setup-secret')->postJson('/install/database', [
        'database' => ['driver' => 'unsupported', 'password' => 'database-secret'],
    ])->assertUnprocessable()->assertJsonValidationErrors('database.driver');
    expect($response->getContent())->not->toContain('database-secret');
});

test('final submission requires chunk warning acknowledgement and permanently closes setup', function (bool $automaticUpdates, bool $redis): void {
    $this->mock(OptimizeInstallation::class)->shouldReceive('handle')->once()->andReturnNull();
    if ($redis) {
        $this->partialMock(CacheConfiguration::class)->shouldReceive('test')->once()->andReturnNull();
    }
    $state = app(InstallationState::class);
    $state->bootstrap(app(EnvironmentWriter::class));
    $environment = Dotenv::createArrayBacked($this->installationDirectory)->load();
    config()->set('app.key', $environment['APP_KEY']);
    app()->instance(ChunkSize::class, new ChunkSize('17', '2M'));
    $this->withHeader('X-Installation-Token', $environment['FILEBEAM_INSTALL_TOKEN']);
    $input = [
        'database' => ['driver' => 'sqlite', 'database' => $this->installationDirectory.'/database/final.sqlite'],
        'instance' => ['name' => 'Our Filebeam', 'url' => 'http://localhost', 'username_domain' => null, 'visibility' => 'public', 'auto_updates_enabled' => $automaticUpdates],
        'storage' => [['name' => 'Private local', 'driver' => 'local', 'root' => 'primary', 'use_path_style_endpoint' => false]],
        'placement_mode' => 'replicate',
        'admin' => ['name' => 'Owner', 'username' => 'owner_admin', 'email' => 'owner@example.test', 'password' => ' correct horse battery staple ', 'password_confirmation' => ' correct horse battery staple ', 'email_ownership_confirmed' => true],
        'chunk_max_size' => 32,
        'cache' => $redis ? ['driver' => 'redis', 'transport' => 'tls', 'host' => 'cache.example.test', 'port' => 6380, 'database' => 0, 'prefix' => 'filebeam_io:', 'password' => 'redis-secret'] : ['driver' => 'file'],
    ];

    $this->postJson('/install/complete', $input)->assertUnprocessable()->assertJsonPath('errors.chunk_warning_acknowledged.0', 'Your server may not support this chunk size');
    $input['chunk_warning_acknowledged'] = true;
    $this->postJson('/install/complete', $input)->assertOk()->assertJsonPath('redirect', 'http://localhost/admin/login');
    expect($state->isCompleted())->toBeTrue();
    expect(DB::connection('installation')->table('users')->count())->toBe(1);
    expect(Hash::check($input['admin']['password'], DB::connection('installation')->table('users')->value('password')))->toBeTrue();
    expect(DB::connection('installation')->table('instance_settings')->where('key', 'anonymous_uploads')->value('value'))->toBe(1);
    $this->postJson('/install/complete', $input)->assertNotFound();
    $this->get('/install')->assertNotFound();
    $values = Dotenv::createArrayBacked($this->installationDirectory)->load();
    expect($values)->toMatchArray(['CHUNK_MAX_SIZE' => '32', 'APP_NAME' => 'Our Filebeam', 'SESSION_DRIVER' => 'cookie', 'FILEBEAM_AUTO_UPDATES_ENABLED' => $automaticUpdates ? 'true' : 'false']);
    expect($values)->not->toHaveKey('FILEBEAM_INSTALL_TOKEN');
    expect($values['CACHE_STORE'])->toBe($redis ? 'redis' : 'file');
    expect($values['QUEUE_CONNECTION'])->toBe('database');
    if ($redis) {
        expect($values)->toMatchArray(['REDIS_HOST' => 'cache.example.test', 'REDIS_SCHEME' => 'tls', 'REDIS_PASSWORD' => 'redis-secret', 'REDIS_CACHE_LOCK_CONNECTION' => 'cache', 'REDIS_CACHE_DB' => '0']);
    }
})->with([[false, false], [true, false], [false, true]]);

test('cache checks require a same-origin setup token and keep secrets out of errors', function (): void {
    app(InstallationState::class)->write(['id' => (string) Str::uuid(), 'status' => 'pending', 'token_hash' => hash('sha256', 'setup-secret')]);
    $this->postJson('/install/cache', ['cache' => ['driver' => 'file']])->assertUnauthorized();
    $this->withHeader('X-Installation-Token', 'setup-secret')->postJson('/install/cache', ['cache' => ['driver' => 'file']])->assertOk();
    $response = $this->postJson('/install/cache', ['cache' => ['driver' => 'unsupported', 'password' => 'redis-secret']])
        ->assertUnprocessable()->assertJsonValidationErrors('cache.driver');
    expect($response->getContent())->not->toContain('redis-secret');
    $this->withHeader('Origin', 'https://elsewhere.example')->postJson('/install/cache', ['cache' => ['driver' => 'file']])->assertForbidden();
});

test('bootstrap will not run over deployment-provided keys', function (): void {
    app()->instance('installation.external_environment', ['APP_KEY' => 'deployment-key']);
    $this->get('/install')->assertNotFound();
    expect(file_exists($this->installationDirectory.'/.env'))->toBeFalse();
});

test('forwarded HTTPS is accepted only from explicitly trusted proxies', function (): void {
    config()->set('app.url', 'http://setup.example.test');
    URL::forceRootUrl('http://setup.example.test');
    $this->withHeader('X-Forwarded-Proto', 'https');
    $this->withServerVariables(['REMOTE_ADDR' => '10.10.0.5']);
    $this->get('http://setup.example.test/install')->assertForbidden();

    config()->set('trustedproxy.proxies', ['10.10.0.5']);
    $page = $this->get('http://setup.example.test/install')->assertOk();
    $this->withHeader('Origin', 'https://setup.example.test')
        ->withHeader('X-Installation-Challenge', $page->viewData('page')['props']['challenge'])
        ->postJson('/install/bootstrap')->assertOk();
    $application = Mockery::mock(Application::class);
    $application->shouldReceive('environment')->with('local')->andReturnFalse();
    $application->shouldReceive('runningUnitTests')->andReturnFalse();
    $request = Request::create('https://attacker.example.test/install');

    try {
        expect(fn (): mixed => (new TrustHosts($application))->handle($request, fn (Request $request): string => $request->getHost()))
            ->toThrow(SuspiciousOperationException::class);
    } finally {
        Request::setTrustedHosts([]);
    }
});
