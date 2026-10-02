<?php

declare(strict_types=1);

use Illuminate\Support\Facades\File;
use Symfony\Component\Process\Process;

test('cached mail configuration preserves environment ownership and secret file values', function (array $overrides, bool $managed): void {
    $directory = storage_path('framework/testing/mail-environment-'.bin2hex(random_bytes(6)));
    File::ensureDirectoryExists($directory);
    File::put($directory.'/password', "operator-password\n");
    $environment = array_fill_keys([
        'MAIL_MAILER', 'MAIL_URL', 'MAIL_SCHEME', 'MAIL_HOST', 'MAIL_PORT', 'MAIL_USERNAME', 'MAIL_PASSWORD',
        'MAIL_PASSWORD_FILE', 'MAIL_EHLO_DOMAIN', 'MAIL_FROM_ADDRESS', 'MAIL_FROM_NAME',
    ], false);
    $environment = array_replace($environment, [
        'APP_ENV' => 'testing',
        'APP_CONFIG_CACHE' => $directory.'/config.php',
        'APP_BASE_PATH' => base_path(),
    ], $overrides);
    if (($environment['MAIL_PASSWORD_FILE'] ?? null) === 'fixture') {
        $environment['MAIL_PASSWORD_FILE'] = $directory.'/password';
    }
    $script = <<<'PHP'
        require 'vendor/autoload.php';
        $app = require 'bootstrap/app.php';
        $app->loadEnvironmentFrom('.env.mail-test-missing');
        $app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();
        file_put_contents($app->getCachedConfigPath(), '<?php return '.var_export(config()->all(), true).';');
        PHP;
    try {
        (new Process([PHP_BINARY, '-r', $script], base_path(), $environment))->mustRun();
        // Read the cache with transport variables absent, as in a cached worker process.
        $read = <<<'PHP'
            require 'vendor/autoload.php';
            $app = require 'bootstrap/app.php';
            $app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();
            config(['database.default' => 'sqlite', 'database.connections.sqlite.database' => ':memory:', 'mail.default' => 'array']);
            app('mail.manager')->mailer();
            echo json_encode(['cached' => $app->configurationIsCached(), 'smtp' => config('smtp'), 'password_matches' => config('mail.mailers.smtp.password') === 'operator-password']);
            PHP;
        $cachedEnvironment = array_replace($environment, array_fill_keys(array_keys($overrides), false));
        $process = (new Process([PHP_BINARY, '-r', $read], base_path(), $cachedEnvironment))->mustRun();
        $result = json_decode($process->getOutput(), true, flags: JSON_THROW_ON_ERROR);
        expect($result['cached'])->toBeTrue()->and($result['smtp']['managed'])->toBe($managed);
        if (isset($overrides['MAIL_PASSWORD_FILE'])) {
            expect($result['password_matches'])->toBeTrue();
        }
        if (isset($overrides['MAIL_FROM_ADDRESS'])) {
            expect($result['smtp']['from_address'])->toBe($overrides['MAIL_FROM_ADDRESS']);
        }
    } finally {
        File::deleteDirectory($directory);
    }
})->with([
    'no transport' => [[], false],
    'partial connection' => [['MAIL_HOST' => 'operator.test'], true],
    'URL' => [['MAIL_URL' => 'smtp://user:password@operator.test:587'], true],
    'non SMTP' => [['MAIL_MAILER' => 'log'], true],
    'sender only' => [['MAIL_FROM_ADDRESS' => 'operator@example.test'], false],
    'file secret' => [['MAIL_PASSWORD_FILE' => 'fixture'], true],
]);

test('container logging emits all standard levels to stdout and respects explicit overrides', function (string $channel): void {
    $directory = storage_path('framework/testing/container-logging-'.bin2hex(random_bytes(6)));
    File::ensureDirectoryExists($directory.'/framework/views');
    $script = <<<'PHP'
        require 'vendor/autoload.php';
        $app = require 'bootstrap/app.php';
        $app->loadEnvironmentFrom('.env.logging-test-missing');
        $app->useStoragePath($argv[1]);
        $app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();
        foreach (['debug', 'info', 'notice', 'warning', 'error', 'critical', 'alert', 'emergency'] as $level) {
            Illuminate\Support\Facades\Log::log($level, 'container-log-{level}', ['level' => $level]);
        }
        PHP;
    try {
        $process = (new Process([PHP_BINARY, '-r', $script, $directory], base_path(), [
            'LOG_CHANNEL' => $channel, 'LOG_STACK' => 'stdout', 'LOG_LEVEL' => 'debug', 'LOG_EMERGENCY_PATH' => 'php://stdout',
            'APP_CONFIG_CACHE' => $directory.'/no-cache.php',
        ]))->mustRun();
        $output = $channel === 'stderr' ? $process->getErrorOutput() : $process->getOutput();
        foreach (['debug', 'info', 'notice', 'warning', 'error', 'critical', 'alert', 'emergency'] as $level) {
            expect($output)->toContain('container-log-'.$level);
        }
        expect(File::exists($directory.'/logs/laravel.log'))->toBeFalse();
    } finally {
        File::deleteDirectory($directory);
    }
})->with(['stdout', 'stack', 'stderr']);
