<?php

declare(strict_types=1);

const BASE_URL = 'http://localhost:8080';

function fail(string $message): never
{
    fwrite(STDERR, "acceptance: {$message}\n");
    exit(1);
}

/** @param list<string> $headers */
function request(string $method, string $path, ?string $body = null, array $headers = []): string
{
    $context = stream_context_create(['http' => [
        'method' => $method,
        'ignore_errors' => true,
        'header' => array_merge(['Accept: application/json', 'Origin: '.BASE_URL], $headers),
        'content' => $body ?? '',
    ]]);
    $response = @file_get_contents(BASE_URL.$path, false, $context);
    preg_match('~\s(\d{3})\s~', $http_response_header[0] ?? '', $match);
    $status = (int) ($match[1] ?? 0);
    if ($status < 200 || $status > 299 || $response === false) {
        $error = $status === 422 && is_string($response) ? json_decode($response, true) : null;
        $details = is_array($error) ? json_encode($error['errors'] ?? [], JSON_THROW_ON_ERROR) : '';
        fail("{$method} {$path} returned HTTP {$status} {$details}");
    }

    return $response;
}

function token(): string
{
    require_once '/opt/filebeam/backend/vendor/autoload.php';
    $values = Dotenv\Dotenv::parse((string) file_get_contents('/data/config/.env'));
    $token = $values['FILEBEAM_INSTALL_TOKEN'] ?? null;
    is_string($token) && $token !== '' || fail('installation token was not written');

    return $token;
}

/** @return list<string> */
function tokenHeaders(): array
{
    return ['Content-Type: application/json', 'X-Installation-Token: '.token()];
}

function bootstrap(): void
{
    $page = request('GET', '/install/');
    preg_match('~"challenge":"([^"]+)"~', $page, $match) || fail('bootstrap challenge was not rendered');
    request('POST', '/install/bootstrap', '{}', ['Content-Type: application/json', 'X-Installation-Challenge: '.$match[1]]);
    token();
}

function complete(): void
{
    $headers = tokenHeaders();
    $configuration = json_decode(request('POST', '/install/configuration', '{}', $headers), true);
    is_array($configuration) || fail('configuration response was not JSON');
    $defaults = $configuration['defaults'] ?? null;
    $chunks = $configuration['chunks'] ?? null;
    is_array($defaults) && is_array($chunks) || fail('configuration response omitted defaults');
    $database = $defaults['database'] ?? null;
    $cache = $defaults['cache'] ?? null;
    is_array($database) && is_array($cache) || fail('configuration response omitted managed defaults');
    $store = ['name' => 'Local storage', 'driver' => 'local', 'root' => 'primary', 'bucket' => '', 'key' => '', 'secret' => '', 'region' => 'us-east-1', 'endpoint' => '', 'use_path_style_endpoint' => false];

    request('POST', '/install/database', json_encode(['database' => $database], JSON_THROW_ON_ERROR), $headers);
    request('POST', '/install/cache', json_encode(['cache' => $cache], JSON_THROW_ON_ERROR), $headers);
    request('POST', '/install/storage', json_encode(['storage' => $store], JSON_THROW_ON_ERROR), $headers);
    $probeSize = $chunks['recommended'] ?? null;
    is_int($probeSize) && $probeSize >= 17 || fail('configuration response has no valid recommended chunk size');
    request('PUT', '/install/probe', str_repeat("\0", $probeSize), ['Content-Type: application/octet-stream', 'Content-Length: '.$probeSize, 'X-Installation-Token: '.token()]);

    $visibility = getenv('FILEBEAM_ACCEPTANCE_VISIBILITY') ?: 'private';
    in_array($visibility, ['private', 'public'], true) || fail('invalid test visibility');
    $payload = ['database' => $database, 'cache' => $cache, 'instance' => ['name' => 'Filebeam acceptance', 'url' => BASE_URL, 'username_domain' => '', 'visibility' => $visibility, 'auto_updates_enabled' => false], 'storage' => [$store], 'placement_mode' => 'replicate', 'admin' => ['name' => 'Acceptance Admin', 'username' => 'acceptance', 'email' => 'acceptance@example.test', 'password' => 'Acceptance-password-123!', 'password_confirmation' => 'Acceptance-password-123!', 'email_ownership_confirmed' => true], 'chunk_max_size' => $probeSize, 'chunk_warning_acknowledged' => true];
    request('POST', '/install/complete', json_encode($payload, JSON_THROW_ON_ERROR), $headers);
}

$mode = $argv[1] ?? '';
match ($mode) {
    'bootstrap' => bootstrap(),
    'complete' => complete(),
    'status' => (function (): void {
        $path = $GLOBALS['argv'][2] ?? '';
        $expected = (int) ($GLOBALS['argv'][3] ?? 0);
        @file_get_contents(BASE_URL.$path);
        preg_match('~\s(\d{3})\s~', $http_response_header[0] ?? '', $match);
        ((int) ($match[1] ?? 0) === $expected) || fail("{$path} did not return HTTP {$expected}");
    })(),
    'status-protected' => (function (): void {
        $path = $GLOBALS['argv'][2] ?? '';
        @file_get_contents(BASE_URL.$path);
        preg_match('~\s(\d{3})\s~', $http_response_header[0] ?? '', $match);
        in_array((int) ($match[1] ?? 0), [403, 404], true) || fail("{$path} was not protected");
    })(),
    'env-equals' => (function (): void {
        require_once '/opt/filebeam/backend/vendor/autoload.php';
        $values = Dotenv\Dotenv::parse((string) file_get_contents('/data/config/.env'));
        (($values[$GLOBALS['argv'][2] ?? ''] ?? null) === ($GLOBALS['argv'][3] ?? null)) || fail('environment value did not match');
    })(),
    'soak' => (function (): void {
        for ($i = 0; $i < 80; $i++) {
            request('GET', '/');
        }
    })(),
    default => fail('unknown mode'),
};
