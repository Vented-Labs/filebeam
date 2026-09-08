<?php

declare(strict_types=1);

use App\Http\Middleware\EnsureAccountIsNotSuspended;
use App\Http\Middleware\HandleInertiaRequests;
use App\Http\Middleware\RequireInstallation;
use App\Http\Middleware\SecurityHeaders;
use App\Http\Middleware\TrackUpdateActivity;
use App\Support\Installation\EnvironmentSettings;
use Illuminate\Foundation\Application;
use Illuminate\Foundation\Configuration\Exceptions;
use Illuminate\Foundation\Configuration\Middleware;
use Illuminate\Http\Middleware\AddLinkHeadersForPreloadedAssets;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;
use Inertia\Inertia;
use Symfony\Component\HttpFoundation\Response;

EnvironmentSettings::resolveFileSecrets();
// Capture deployment overrides before Dotenv populates the process environment.
$installationEnvironment = array_intersect_key(array_replace(getenv(), $_SERVER), array_flip(EnvironmentSettings::captureKeys()));

$application = Application::configure(basePath: dirname(__DIR__))
    ->withRouting(
        web: __DIR__.'/../routes/web.php',
        api: __DIR__.'/../routes/api.php',
        commands: __DIR__.'/../routes/console.php',
        health: '/up',
        then: function (): void {
            Route::group([], __DIR__.'/../routes/install.php');
        },
    )
    ->withMiddleware(function (Middleware $middleware): void {
        $middleware->prepend(RequireInstallation::class);
        $middleware->append(TrackUpdateActivity::class);
        $middleware->trimStrings(except: [fn (Request $request): bool => $request->is('install/*')]);
        $middleware->trustHosts(function (): array {
            $applicationHost = parse_url((string) config('app.url'), PHP_URL_HOST);
            $usernameDomain = config('filebeam.username_domain');

            return array_values(array_filter([
                is_string($applicationHost) ? '^'.preg_quote($applicationHost, '/').'$' : null,
                is_string($usernameDomain) && $usernameDomain !== '' ? '^'.preg_quote($usernameDomain, '/').'$' : null,
            ]));
        }, subdomains: false);
        $middleware->append(SecurityHeaders::class);
        $middleware->web(append: [
            EnsureAccountIsNotSuspended::class,
            HandleInertiaRequests::class,
            AddLinkHeadersForPreloadedAssets::class,
        ]);
        $middleware->api(append: [EnsureAccountIsNotSuspended::class]);
    })
    ->withExceptions(function (Exceptions $exceptions): void {
        $exceptions->shouldRenderJsonWhen(
            fn (Request $request) => $request->is('api/*') || $request->expectsJson(),
        );
        $exceptions->respond(function (Response $response, Throwable $exception, Request $request): Response {
            $status = $response->getStatusCode();

            if (
                config('app.debug')
                || ! $request->header('X-Inertia')
                || $request->is('api/*')
                || $request->expectsJson()
                || $status < 400
                || $status >= 600
            ) {
                return $response;
            }

            $errorResponse = Inertia::render('Error', [
                'status' => $status,
                'branding' => [
                    ...config('filebeam.branding'),
                    'default_logo_url' => asset('brand/filebeam-logo-header.svg'),
                    'default_mark_url' => asset('brand/filebeam-mark.svg'),
                ],
            ])->toResponse($request)->setStatusCode($status);

            if ($response->headers->has('Retry-After')) {
                $errorResponse->headers->set('Retry-After', $response->headers->get('Retry-After'));
            }

            return $errorResponse;
        });
    })->create();

if (filter_var(getenv('FILEBEAM_CONTAINER'), FILTER_VALIDATE_BOOL)) {
    $dataDirectory = EnvironmentSettings::containerDataDirectory();
    $application->useEnvironmentPath($dataDirectory.'/config')->loadEnvironmentFrom('.env');
    // Durable Laravel state belongs on the data volume; only generated caches live in /run.
    $application->useStoragePath($dataDirectory.'/app');
    foreach ([
        'APP_CONFIG_CACHE' => '/run/filebeam/cache/config.php',
        'APP_ROUTES_CACHE' => '/run/filebeam/cache/routes-v7.php',
        'APP_EVENTS_CACHE' => '/run/filebeam/cache/events.php',
        'APP_SERVICES_CACHE' => '/run/filebeam/cache/services.php',
        'APP_PACKAGES_CACHE' => '/run/filebeam/cache/packages.php',
        'VIEW_COMPILED_PATH' => '/run/filebeam/views',
        'FILEBEAM_FILESTORE_ROOT' => '/storage',
    ] as $key => $value) {
        if (getenv($key) === false) {
            putenv($key.'='.$value);
            $_ENV[$key] = $value;
            $_SERVER[$key] = $value;
        }
    }
    if (getenv('FILEBEAM_VARIANT') === 'omnibus') {
        foreach ([
            'DB_CONNECTION' => 'pgsql',
            'DB_HOST' => '/run/filebeam/postgresql',
            'DB_PORT' => '5432',
            'DB_DATABASE' => 'filebeam',
            'DB_USERNAME' => 'filebeam',
            'DB_PASSWORD' => '',
            'DB_SOCKET' => '',
            'CACHE_STORE' => 'redis',
            'REDIS_CLIENT' => 'phpredis',
            'REDIS_HOST' => '/run/filebeam/valkey/valkey.sock',
            'REDIS_PORT' => '0',
            'REDIS_USERNAME' => '',
            'REDIS_DB' => '0',
            'REDIS_CACHE_DB' => '1',
            'REDIS_CACHE_CONNECTION' => 'cache',
            'REDIS_CACHE_LOCK_CONNECTION' => 'cache',
            'QUEUE_CONNECTION' => 'redis',
            'FILEBEAM_CRON_QUEUE_ENABLED' => 'false',
        ] as $key => $value) {
            putenv($key.'='.$value);
            $_ENV[$key] = $value;
            $_SERVER[$key] = $value;
        }
        // A deployment URL would otherwise override the fixed socket configuration.
        putenv('DB_URL');
        putenv('REDIS_URL');
        unset($_ENV['DB_URL'], $_SERVER['DB_URL'], $_ENV['REDIS_URL'], $_SERVER['REDIS_URL']);
    }
}

$application->instance('installation.external_environment', $installationEnvironment);

return $application;
