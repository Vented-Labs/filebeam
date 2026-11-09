<?php

declare(strict_types=1);

namespace Tests\Support;

use Illuminate\Contracts\Console\Kernel;
use Illuminate\Foundation\Application;
use Illuminate\Support\Facades\DB;
use LogicException;

final class ExternalDeviceRace
{
    /** @return array{connection: string, config: array<string, mixed>}|null */
    public static function connection(Application $app): ?array
    {
        $driver = getenv('FILEBEAM_EXTERNAL_DB_CONNECTION');
        $url = getenv('FILEBEAM_EXTERNAL_DB_URL');
        if ($driver === false && $url === false) {
            return null;
        }
        if (! is_string($driver) || ! in_array($driver, ['mysql', 'mariadb', 'pgsql'], true) || ! is_string($url) || $url === '') {
            throw new LogicException('FILEBEAM_EXTERNAL_DB_CONNECTION and FILEBEAM_EXTERNAL_DB_URL must select mysql, mariadb, or pgsql.');
        }

        $config = $app['config']->get("database.connections.{$driver}");
        assert(is_array($config));
        $config['url'] = $url;
        $config['database'] = null;
        $name = 'external_device_race';
        $app['config']->set("database.connections.{$name}", $config);
        DB::purge($name);

        $database = DB::connection($name)->getDatabaseName();
        if ($database !== 'filebeam_testing') {
            throw new LogicException('External device race tests only run against the disposable filebeam_testing database.');
        }

        return ['connection' => $name, 'config' => $config];
    }

    /** @param array<string, mixed> $config */
    public static function migrate(Application $app, string $connection, array $config): void
    {
        $app['config']->set("database.connections.{$connection}", $config);
        DB::purge($connection);
        $app->make(Kernel::class)->call('migrate:fresh', ['--database' => $connection, '--force' => true]);
    }

    /** @param array<string, mixed> $payload @return array{process: resource, pipes: array<int, resource>} */
    public static function worker(string $mode, array $payload, string $barrier): array
    {
        $command = [PHP_BINARY, base_path('tests/Support/external-device-race-worker.php'), $mode, base64_encode(json_encode($payload, JSON_THROW_ON_ERROR)), $barrier];
        $process = proc_open($command, [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
        if (! is_resource($process)) {
            throw new LogicException('Unable to start external device race worker.');
        }

        return ['process' => $process, 'pipes' => $pipes];
    }

    /** @param list<array{process: resource, pipes: array<int, resource>}> $workers @return list<array{status: int, error: string|null}> */
    public static function release(array $workers, string $barrier): array
    {
        $deadline = microtime(true) + 15;
        while (count(glob($barrier.'/*.ready') ?: []) < count($workers) && microtime(true) < $deadline) {
            usleep(10_000);
        }
        if (count(glob($barrier.'/*.ready') ?: []) !== count($workers)) {
            throw new LogicException('External device race workers did not reach the start barrier.');
        }
        touch($barrier.'/release');

        $results = [];
        foreach ($workers as $worker) {
            $output = stream_get_contents($worker['pipes'][1]);
            $error = stream_get_contents($worker['pipes'][2]);
            fclose($worker['pipes'][1]);
            fclose($worker['pipes'][2]);
            $exit = proc_close($worker['process']);
            $result = json_decode($output, true);
            $results[] = is_array($result) && isset($result['status'])
                ? ['status' => (int) $result['status'], 'error' => isset($result['error']) ? (string) $result['error'] : null]
                : ['status' => $exit, 'error' => trim($error) !== '' ? trim($error) : 'Worker returned no result.'];
        }

        return $results;
    }
}
