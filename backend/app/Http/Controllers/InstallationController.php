<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Support\Installation\CacheConfiguration;
use App\Support\Installation\ChunkSize;
use App\Support\Installation\CompleteInstallation;
use App\Support\Installation\ContainerConfiguration;
use App\Support\Installation\EnvironmentWriter;
use App\Support\Installation\InstallationConfiguration;
use App\Support\Installation\InstallationState;
use Illuminate\Cache\FileStore;
use Illuminate\Cache\RateLimiter;
use Illuminate\Cache\Repository;
use Illuminate\Filesystem\Filesystem;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\File;
use Illuminate\Validation\ValidationException;
use Inertia\Inertia;
use Inertia\Response;
use PDO;

class InstallationController extends Controller
{
    public function show(InstallationState $state): Response
    {
        $bootstrap = $state->canBootstrap();

        return Inertia::render('Install', [
            'bootstrapRequired' => $bootstrap,
            'challenge' => $bootstrap ? $state->challenge() : null,
            'unavailableReason' => null,
        ]);
    }

    public function bootstrap(InstallationState $state, EnvironmentWriter $writer): JsonResponse
    {
        $state->bootstrap($writer);

        return response()->json(['message' => 'Open the .env file on your server and copy FILEBEAM_INSTALL_TOKEN into the installer. The token is never displayed in your browser.']);
    }

    public function configuration(Request $request, InstallationState $state, ChunkSize $chunks, CacheConfiguration $cache, ContainerConfiguration $container): JsonResponse
    {
        $drivers = array_values(array_filter(['sqlite', 'mysql', 'mariadb', 'pgsql'], fn (string $driver): bool => in_array($driver === 'mariadb' ? 'mysql' : $driver, PDO::getAvailableDrivers(), true)));
        $containerDefaults = $container->publicDefaults();

        return response()->json([
            'databaseDrivers' => $drivers,
            'redisAvailable' => extension_loaded('redis'),
            'defaults' => [
                'cache' => array_replace($containerDefaults['cache'], ['prefix' => $containerDefaults['cache']['prefix'] === 'filebeam:' ? 'filebeam:'.substr((string) ($state->read()['id'] ?? 'instance'), 0, 12).':' : $containerDefaults['cache']['prefix']]),
                'database' => $container->enabled() ? $containerDefaults['database'] : ['driver' => $drivers[0] ?? 'sqlite', 'transport' => 'tcp', 'socket' => '', 'host' => '127.0.0.1', 'port' => 3306, 'database' => (string) config('installation.sqlite_directory').'/database.sqlite', 'username' => '', 'password' => '', 'sslmode' => 'prefer'],
                'instance' => array_replace(['name' => 'Filebeam', 'url' => $request->getSchemeAndHttpHost(), 'username_domain' => '', 'visibility' => 'private', 'auto_updates_enabled' => false], $containerDefaults['instance']),
                'storage' => [['name' => 'Local storage', 'driver' => 'local', 'root' => 'primary', 'bucket' => '', 'key' => '', 'secret' => '', 'region' => 'us-east-1', 'endpoint' => '', 'use_path_style_endpoint' => false]],
                'placement_mode' => 'replicate',
            ],
            'managed' => ['container' => $container->enabled(), 'variant' => $container->variant(), ...$container->managed()],
            'prerequisites' => $this->prerequisites($state),
            'chunks' => $chunks->limits(),
        ]);
    }

    /** @return list<array{label: string, passed: bool}> */
    private function prerequisites(InstallationState $state): array
    {
        return [
            ['label' => 'At least one supported PDO driver', 'passed' => array_intersect(['sqlite', 'mysql', 'pgsql'], PDO::getAvailableDrivers()) !== []],
            ['label' => 'Application key initialized', 'passed' => is_string(config('app.key')) && config('app.key') !== ''],
            ['label' => 'Environment file and parent directory writable', 'passed' => $this->writableDirectory(dirname($state->environmentPath()))],
            ['label' => 'Persistent installation state writable', 'passed' => $this->writableDirectory($state->directory())],
            ['label' => 'Runtime storage writable', 'passed' => $this->writableDirectory(storage_path('framework'))],
            ['label' => 'Bootstrap cache directory writable', 'passed' => $this->writableDirectory(dirname(app()->getCachedConfigPath()))],
        ];
    }

    private function writableDirectory(string $path): bool
    {
        try {
            File::ensureDirectoryExists($path, 0700, true);

            return is_dir($path) && is_writable($path);
        } catch (\Throwable) {
            return false;
        }
    }

    public function database(Request $request, InstallationState $state, InstallationConfiguration $configuration, ContainerConfiguration $container): JsonResponse
    {
        $data = $request->validate(['database' => ['required', 'array']]);
        $state->locked(function () use ($state, $configuration, $container, $data): void {
            abort_unless($state->isPending(), 404);
            $configuration->testDatabase($container->apply(['database' => $data['database']])['database']);
        });

        return response()->json(['message' => 'Database connection and schema permissions verified.']);
    }

    public function storage(Request $request, InstallationState $state, InstallationConfiguration $configuration): JsonResponse
    {
        $data = $request->validate(['storage' => ['required', 'array']]);
        $state->locked(function () use ($state, $configuration, $data): void {
            abort_unless($state->isPending(), 404);
            $configuration->testStorage($data['storage']);
        });

        return response()->json(['message' => 'Storage write, read, and delete verified.']);
    }

    public function cache(Request $request, InstallationState $state, CacheConfiguration $configuration, ContainerConfiguration $container): JsonResponse
    {
        $data = $request->validate(['cache' => ['required', 'array']]);
        $state->locked(function () use ($state, $configuration, $container, $data): void {
            abort_unless($state->isPending(), 404);
            $configuration->test($container->apply(['cache' => $data['cache']])['cache']);
        });

        return response()->json(['message' => 'Cache reads, writes, counters, deletion, and exclusive locks verified.']);
    }

    public function probe(Request $request, InstallationState $state): JsonResponse
    {
        abort_unless($request->header('Content-Type') === 'application/octet-stream', 415);
        $length = $request->header('Content-Length');
        abort_unless(is_string($length) && ctype_digit($length), 411);
        abort_if((int) $length > 25_000_000, 413);
        abort_if((int) $length < 17, 422);

        $limiter = new RateLimiter(new Repository(new FileStore(new Filesystem, $state->directory().'/rate-limits')));
        $key = 'probes:'.($state->read()['id'] ?? '');
        abort_if($limiter->tooManyAttempts($key, 5), 429, 'Chunk probe budget exhausted. Retry in a minute.');
        $limiter->hit($key, 60);
        $stream = $request->getContent(true);
        $bytes = 0;
        try {
            while (! feof($stream)) {
                $part = fread($stream, min(8192, 25_000_001 - $bytes));
                abort_if($part === false || ($part === '' && ! feof($stream)), 400);
                $bytes += strlen($part);
                abort_if($bytes > 25_000_000, 413);
            }
        } finally {
            fclose($stream);
        }
        abort_unless($bytes === (int) $length, 422, 'The complete probe body was not received.');

        return response()->json(['bytes' => $bytes]);
    }

    public function complete(Request $request, InstallationState $state, InstallationConfiguration $configuration, CompleteInstallation $complete, ChunkSize $chunks, ContainerConfiguration $container): JsonResponse
    {
        foreach ($this->prerequisites($state) as $prerequisite) {
            if (! $prerequisite['passed']) {
                throw ValidationException::withMessages(['installation' => 'Resolve the failed prerequisite: '.$prerequisite['label']]);
            }
        }
        $validated = $configuration->validate($container->apply($request->all()));
        if ($chunks->warning($validated['chunk_max_size']) && ! ($validated['chunk_warning_acknowledged'] ?? false)) {
            throw ValidationException::withMessages(['chunk_warning_acknowledged' => 'Your server may not support this chunk size']);
        }
        $complete->handle($validated);

        return response()->json(['message' => 'Filebeam is installed. Sign in with your administrator account.', 'redirect' => rtrim($validated['instance']['url'], '/').'/admin/login']);
    }
}
