<?php

declare(strict_types=1);

namespace Filebeam\Updater;

use PDO;
use RuntimeException;

final class PostgresBackup
{
    private const COMMAND_TIMEOUT = 15;

    private const BACKUP_TIMEOUT = 300;

    /** @var array{host: string, port: string, database: string, username: string, password: string, sslmode: string, sslrootcert: ?string, sslcert: ?string, sslkey: ?string, application_name: ?string} */
    private readonly array $settings;

    /** @param array<string, mixed> $configuration */
    public function __construct(
        array $configuration,
        private readonly string $directory,
        private readonly string $dumpBinary = 'pg_dump',
        private readonly string $restoreBinary = 'pg_restore',
        private readonly int $timeout = self::BACKUP_TIMEOUT,
    ) {
        if ($timeout < 1 || $timeout > 3600) {
            throw new RuntimeException('PostgreSQL backup timeout is invalid.');
        }

        $this->settings = $this->normalize($configuration);
    }

    /**
     * PostgreSQL client tools must exactly match the server major version.
     * This intentionally rejects otherwise-supported cross-major pg_dump use.
     */
    public function preflight(): void
    {
        if (! extension_loaded('pdo_pgsql')) {
            throw new RuntimeException('The pdo_pgsql PHP extension is required for PostgreSQL backups.');
        }
        foreach (['PGSERVICE', 'PGSERVICEFILE', 'PGHOSTADDR', 'PGOPTIONS'] as $key) {
            if (getenv($key) !== false) {
                throw new RuntimeException('PostgreSQL client environment is unsafe for backup preflight.');
            }
        }

        try {
            $pdo = new PDO($this->dsn(), $this->settings['username'], $this->settings['password'], [
                PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION,
                PDO::ATTR_TIMEOUT => 10,
            ]);
            $statement = $pdo->query('SHOW server_version_num');
            $version = $statement === false ? false : $statement->fetchColumn();
        } catch (\Throwable) {
            throw new RuntimeException('Unable to connect to PostgreSQL for backup preflight.');
        }

        if (is_int($version)) {
            $version = (string) $version;
        }
        if (! is_string($version) || preg_match('/^\d{5,6}$/', $version) !== 1) {
            throw new RuntimeException('PostgreSQL returned an invalid server version.');
        }

        $serverMajor = (int) substr($version, 0, strlen($version) - 4);
        if ($serverMajor < 10) {
            throw new RuntimeException('PostgreSQL 10 or newer is required for automated backups.');
        }
        foreach ([$this->dumpBinary, $this->restoreBinary] as $binary) {
            $clientMajor = $this->commandMajor($binary);
            if ($clientMajor !== $serverMajor) {
                throw new RuntimeException('PostgreSQL client tools must match the server major version.');
            }
        }
    }

    public function backup(): string
    {
        $this->ensurePrivateDirectory();
        $name = 'database-'.gmdate('YmdHis').'-'.bin2hex(random_bytes(8)).'.dump';
        $target = $this->directory.'/'.$name;
        $partial = $target.'.partial';
        $passfile = $this->directory.'/.pgpass-'.bin2hex(random_bytes(8));
        $partialCreated = false;

        try {
            $this->createPrivateFile($partial);
            $partialCreated = true;
            $this->writePassfile($passfile);
            $environment = $this->environment($passfile);
            $this->run([
                $this->dumpBinary,
                '--format=custom',
                '--no-password',
                '--file='.$partial,
                '--host='.$this->settings['host'],
                '--port='.$this->settings['port'],
                '--username='.$this->settings['username'],
                '--dbname='.$this->settings['database'],
            ], $environment, $this->timeout, 'pg_dump backup');

            $header = file_get_contents($partial, false, null, 0, 5);
            if ($header !== 'PGDMP') {
                throw new RuntimeException('PostgreSQL backup validation failed.');
            }

            // Listing validates the archive catalog; generating SQL to /dev/null reads its data sections too.
            $this->run([$this->restoreBinary, '--list', $partial], $environment, self::COMMAND_TIMEOUT, 'pg_restore archive catalog validation');
            $this->run([$this->restoreBinary, '--file=/dev/null', $partial], $environment, $this->timeout, 'pg_restore archive data validation');

            if (! rename($partial, $target)) {
                throw new RuntimeException('Unable to finalize PostgreSQL backup.');
            }
            chmod($target, 0600);

            return $target;
        } catch (\Throwable $exception) {
            if ($partialCreated && (is_file($partial) || is_link($partial))) {
                @unlink($partial);
            }

            throw $exception instanceof RuntimeException ? $exception : new RuntimeException('PostgreSQL backup failed.');
        } finally {
            if (is_file($passfile) || is_link($passfile)) {
                @unlink($passfile);
            }
        }
    }

    /**
     * @param  array<string, mixed>  $configuration
     * @return array{host: string, port: string, database: string, username: string, password: string, sslmode: string, sslrootcert: ?string, sslcert: ?string, sslkey: ?string, application_name: ?string}
     */
    private function normalize(array $configuration): array
    {
        if (($configuration['driver'] ?? 'pgsql') !== 'pgsql' || ($configuration['url'] ?? '') !== '') {
            throw new RuntimeException('Invalid PostgreSQL backup configuration.');
        }

        $socket = $configuration['unix_socket'] ?? $configuration['socket'] ?? null;
        $host = is_string($socket) && $socket !== '' ? $socket : ($configuration['host'] ?? '127.0.0.1');
        $port = (string) ($configuration['port'] ?? '5432');
        $database = $configuration['database'] ?? null;
        $username = $configuration['username'] ?? null;
        $password = $configuration['password'] ?? '';

        if (! is_string($host) || ! is_string($database) || ! is_string($username) || ! is_string($password)
            || ! preg_match('/^[1-9][0-9]{0,4}$/', $port) || (int) $port > 65535
            || ! $this->safeConnectionValue($host) || ! $this->safeConnectionValue($database) || ! $this->safeConnectionValue($username)
            || $database === '' || $username === '' || str_contains(strtolower($database), 'postgresql://') || str_contains(strtolower($database), 'postgres://')
            || str_contains($password, "\0") || str_contains($password, "\n") || str_contains($password, "\r")) {
            throw new RuntimeException('Invalid PostgreSQL backup configuration.');
        }

        $sslmode = (string) ($configuration['sslmode'] ?? 'prefer');
        if (! in_array($sslmode, ['disable', 'allow', 'prefer', 'require', 'verify-ca', 'verify-full'], true)) {
            throw new RuntimeException('Invalid PostgreSQL SSL mode.');
        }

        $settings = [
            'host' => $host,
            'port' => $port,
            'database' => $database,
            'username' => $username,
            'password' => $password,
            'sslmode' => $sslmode,
        ];
        foreach (['sslrootcert', 'sslcert', 'sslkey', 'application_name'] as $key) {
            $value = $configuration[$key] ?? null;
            if ($value !== null && (! is_string($value) || ! $this->safeConnectionValue($value))) {
                throw new RuntimeException('Invalid PostgreSQL backup configuration.');
            }
            $settings[$key] = $value;
        }

        return $settings;
    }

    private function safeConnectionValue(string $value): bool
    {
        return $value !== '' && ! preg_match('/[\x00-\x1f;=]/', $value);
    }

    private function dsn(): string
    {
        $parts = ['host='.$this->settings['host'], 'port='.$this->settings['port'], 'dbname='.$this->settings['database'], 'connect_timeout=10', 'sslmode='.$this->settings['sslmode']];
        foreach (['sslrootcert', 'sslcert', 'sslkey', 'application_name'] as $key) {
            if ($this->settings[$key] !== null) {
                $parts[] = $key.'='.$this->settings[$key];
            }
        }

        return 'pgsql:'.implode(';', $parts);
    }

    private function commandMajor(string $binary): int
    {
        $result = $this->run([$binary, '--version'], $this->environment(null), self::COMMAND_TIMEOUT, basename($binary).' version check', true);
        if (preg_match('/\b(\d+)(?:\.\d+){0,2}\b/', $result, $matches) !== 1) {
            throw new RuntimeException('Unable to determine PostgreSQL client tool version.');
        }

        return (int) $matches[1];
    }

    /** @return array<string, string> */
    private function environment(?string $passfile): array
    {
        $environment = getenv();
        foreach (array_keys($environment) as $key) {
            if (str_starts_with($key, 'PGSSL')) {
                unset($environment[$key]);
            }
        }
        foreach (['PGPASSWORD', 'PGPASSFILE', 'PGHOST', 'PGHOSTADDR', 'PGPORT', 'PGUSER', 'PGDATABASE', 'PGAPPNAME', 'PGSERVICE', 'PGSERVICEFILE', 'PGOPTIONS', 'PGCONNECT_TIMEOUT', 'PGTARGETSESSIONATTRS', 'PGCHANNELBINDING', 'PGGSSENCMODE', 'PGREQUIREAUTH', 'PGLOADBALANCEHOSTS'] as $key) {
            unset($environment[$key]);
        }
        if ($passfile !== null) {
            $environment['PGPASSFILE'] = $passfile;
        }
        $environment['PGSSLMODE'] = $this->settings['sslmode'];
        foreach (['sslrootcert' => 'PGSSLROOTCERT', 'sslcert' => 'PGSSLCERT', 'sslkey' => 'PGSSLKEY', 'application_name' => 'PGAPPNAME'] as $setting => $variable) {
            if ($this->settings[$setting] !== null) {
                $environment[$variable] = $this->settings[$setting];
            }
        }

        return $environment;
    }

    private function ensurePrivateDirectory(): void
    {
        if ($this->directory === '' || ! str_starts_with($this->directory, '/')) {
            throw new RuntimeException('PostgreSQL backup directory must be absolute.');
        }
        $this->assertNoSymlinks($this->directory);
        if (! is_dir($this->directory) && ! mkdir($this->directory, 0700, true) && ! is_dir($this->directory)) {
            throw new RuntimeException('Unable to create PostgreSQL backup directory.');
        }
        $this->assertNoSymlinks($this->directory);
        if (! chmod($this->directory, 0700)) {
            throw new RuntimeException('Unable to secure PostgreSQL backup directory.');
        }
    }

    private function assertNoSymlinks(string $path): void
    {
        $current = '';
        foreach (explode('/', ltrim($path, '/')) as $segment) {
            $current .= '/'.$segment;
            if (is_link($current)) {
                throw new RuntimeException('PostgreSQL backup path must not contain symbolic links.');
            }
        }
    }

    private function createPrivateFile(string $path): void
    {
        $file = @fopen($path, 'x');
        if ($file === false) {
            throw new RuntimeException('Unable to create PostgreSQL backup.');
        }
        if (! chmod($path, 0600)) {
            fclose($file);
            @unlink($path);

            throw new RuntimeException('Unable to create PostgreSQL backup.');
        }
        fclose($file);
    }

    private function writePassfile(string $path): void
    {
        $this->createPrivateFile($path);
        $line = implode(':', array_map($this->escapePassfileField(...), [$this->settings['host'], $this->settings['port'], $this->settings['database'], $this->settings['username'], $this->settings['password']]))."\n";
        if (file_put_contents($path, $line) === false) {
            throw new RuntimeException('Unable to create PostgreSQL credentials file.');
        }
    }

    private function escapePassfileField(string $value): string
    {
        return str_replace(['\\', ':'], ['\\\\', '\\:'], $value);
    }

    /**
     * @param  list<string>  $command
     * @param  array<string, string>  $environment
     */
    private function run(array $command, array $environment, int $timeout, string $operation, bool $captureOutput = false): string
    {
        $process = @proc_open($command, [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes, null, $environment, ['bypass_shell' => true]);
        if (! is_resource($process)) {
            throw new RuntimeException('Required PostgreSQL client tool is unavailable.');
        }
        fclose($pipes[0]);
        stream_set_blocking($pipes[1], false);
        stream_set_blocking($pipes[2], false);
        $output = '';
        $error = '';
        // Keep enough lookahead to redact a credential crossing the displayed boundary.
        $errorLimit = 65536 + 3 * strlen($this->settings['password']);
        $deadline = microtime(true) + $timeout;
        $reportedExit = -1;
        $exit = -1;
        try {
            while (true) {
                $read = [$pipes[1], $pipes[2]];
                $write = null;
                $except = null;
                @stream_select($read, $write, $except, 0, 200000);
                foreach ($read as $stream) {
                    $chunk = fread($stream, 8192);
                    if ($stream === $pipes[2] && $chunk !== false && strlen($error) < $errorLimit) {
                        $error .= substr($chunk, 0, $errorLimit - strlen($error));
                    }
                    if ($stream === $pipes[1] && $captureOutput && $chunk !== false && strlen($output) < 65536) {
                        $output .= substr($chunk, 0, 65536 - strlen($output));
                    }
                }
                $status = proc_get_status($process);
                if (! $status['running']) {
                    $reportedExit = $status['exitcode'];

                    break;
                }
                if (microtime(true) >= $deadline) {
                    proc_terminate($process, 9);
                    throw new RuntimeException('PostgreSQL client tool timed out. Operation: '.$operation.'.'.$this->diagnostic($error));
                }
            }
            foreach ([$pipes[1], $pipes[2]] as $pipe) {
                while (! feof($pipe)) {
                    $chunk = fread($pipe, 8192);
                    if ($chunk === false || $chunk === '') {
                        break;
                    }
                    if ($pipe === $pipes[2] && strlen($error) < $errorLimit) {
                        $error .= substr($chunk, 0, $errorLimit - strlen($error));
                    }
                    if ($pipe === $pipes[1] && $captureOutput && strlen($output) < 65536) {
                        $output .= substr($chunk, 0, 65536 - strlen($output));
                    }
                }
            }
        } finally {
            fclose($pipes[1]);
            fclose($pipes[2]);
            $exit = proc_close($process);
        }
        if ($exit === -1 && $reportedExit >= 0) {
            $exit = $reportedExit;
        }
        if ($exit !== 0) {
            throw new RuntimeException('PostgreSQL client tool failed. Operation: '.$operation.'; exit code: '.$exit.'.'.$this->diagnostic($error));
        }

        return $output;
    }

    private function diagnostic(string $error): string
    {
        $password = $this->settings['password'];
        if ($password !== '') {
            $secrets = array_unique([$password, rawurlencode($password), urlencode($password), $this->escapePassfileField($password)]);
            usort($secrets, static fn (string $a, string $b): int => strlen($b) <=> strlen($a));
            $error = str_replace($secrets, '[redacted]', $error);
        }
        // Keep diagnostics printable and JSON-safe for status.json, even with binary stderr.
        $error = trim(preg_replace('/[^\x20-\x7e\n\t]/', '?', $error) ?? '');
        if ($error === '') {
            return '';
        }

        return ' stderr: '.substr($error, 0, 4096).(strlen($error) > 4096 ? ' [truncated]' : '');
    }
}
