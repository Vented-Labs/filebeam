<?php

declare(strict_types=1);

const BASE_URL = 'http://localhost:8080';

function fail(string $message): never
{
    fwrite(STDERR, "worker-isolation: {$message}\n");
    exit(1);
}

function bootstrapApp(): void
{
    require_once '/opt/filebeam/backend/vendor/autoload.php';
    $app = require '/opt/filebeam/backend/bootstrap/app.php';
    $app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();
}

function seed(): void
{
    bootstrapApp();

    foreach ([
        ['Worker A', 'worker-a', 'worker-a@example.test', 'Worker-A-password-123!'],
        ['Worker B', 'worker-b', 'worker-b@example.test', 'Worker-B-password-123!'],
    ] as [$name, $username, $email, $password]) {
        App\Models\User::query()->updateOrCreate(
            ['email' => $email],
            [
                'name' => $name,
                'username' => $username,
                'normalized_username' => $username,
                'email_verified_at' => now(),
                'password' => Illuminate\Support\Facades\Hash::make($password),
                'role' => 'user',
                'inbox_enabled' => false,
                'notification_channel' => 'mail',
            ],
        );
    }
}

/** @param array<string, string> $cookies @param list<string> $headers @return array{status: int, body: string} */
function request(string $method, string $path, array &$cookies, ?string $body = null, array $headers = []): array
{
    $curl = curl_init(BASE_URL.$path);
    $cookieHeader = implode('; ', array_map(fn (string $name): string => $name.'='.$cookies[$name], array_keys($cookies)));
    curl_setopt_array($curl, [
        CURLOPT_CUSTOMREQUEST => $method,
        CURLOPT_POSTFIELDS => $body,
        CURLOPT_HTTPHEADER => array_merge(['Accept: text/html, application/xhtml+xml', 'Origin: '.BASE_URL], $cookieHeader === '' ? [] : ['Cookie: '.$cookieHeader], $headers),
        CURLOPT_HEADER => true,
        CURLOPT_RETURNTRANSFER => true,
        CURLOPT_FOLLOWLOCATION => false,
        CURLOPT_TIMEOUT => 15,
    ]);
    $response = curl_exec($curl);
    $status = (int) curl_getinfo($curl, CURLINFO_RESPONSE_CODE);
    $headerSize = (int) curl_getinfo($curl, CURLINFO_HEADER_SIZE);
    $error = curl_error($curl);
    is_string($response) || fail("{$method} {$path} failed: {$error}");

    $rawHeaders = substr($response, 0, $headerSize);
    foreach (preg_split('/\r\n/', $rawHeaders) ?: [] as $header) {
        if (preg_match('/^Set-Cookie:\s*([^=;]+)=([^;]*)/i', $header, $match)) {
            $cookies[$match[1]] = $match[2];
        }
    }

    return ['status' => $status, 'body' => substr($response, $headerSize)];
}

/** @param array<string, string> $cookies */
function login(array &$cookies, string $email, string $password): string
{
    $page = request('GET', '/login', $cookies);
    $page['status'] === 200 || fail("GET /login returned HTTP {$page['status']}");
    preg_match('~<script data-page="app" type="application/json">(.+?)</script>~s', $page['body'], $match) || fail('GET /login did not render Inertia page data');
    try {
        $inertia = json_decode($match[1], true, flags: JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        fail('GET /login rendered invalid Inertia page data');
    }
    $version = $inertia['version'] ?? null;
    is_string($version) && $version !== '' || fail('GET /login omitted the Inertia version');
    $xsrf = $cookies['XSRF-TOKEN'] ?? null;
    is_string($xsrf) && $xsrf !== '' || fail('GET /login did not set an XSRF-TOKEN cookie');
    $response = request('POST', '/login', $cookies, http_build_query(['email' => $email, 'password' => $password]), [
        'Content-Type: application/x-www-form-urlencoded',
        'Referer: '.BASE_URL.'/login',
        'X-XSRF-TOKEN: '.urldecode($xsrf),
    ]);
    $response['status'] === 302 || fail("POST /login for {$email} returned HTTP {$response['status']}");

    return $version;
}

/** @param array<string, string> $cookies */
function assertIdentity(array &$cookies, string $username, string $version): void
{
    $response = request('GET', '/account', $cookies, null, ['X-Inertia: true', 'X-Inertia-Version: '.$version, 'X-Requested-With: XMLHttpRequest']);
    $response['status'] === 200 || fail("GET /account for {$username} returned HTTP {$response['status']}");
    try {
        $page = json_decode($response['body'], true, flags: JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        fail("GET /account for {$username} did not return Inertia JSON");
    }
    $actual = $page['props']['user']['username'] ?? null;
    $actual === $username || fail("identity leak: expected {$username}, received ".var_export($actual, true));
}

function isolation(int $iterations): void
{
    extension_loaded('curl') || fail('the production image PHP runtime does not have ext-curl');
    $admin = $a = $b = $anonymous = [];
    $version = login($admin, 'acceptance@example.test', 'Acceptance-password-123!');
    login($a, 'worker-a@example.test', 'Worker-A-password-123!');
    login($b, 'worker-b@example.test', 'Worker-B-password-123!');

    // Warm every session before measuring the long-lived worker under alternating identities.
    for ($i = 0; $i < $iterations; $i++) {
        assertIdentity($admin, 'acceptance', $version);
        assertIdentity($a, 'worker-a', $version);
        assertIdentity($b, 'worker-b', $version);
        $response = request('GET', '/account', $anonymous, null, ['X-Inertia: true', 'X-Inertia-Version: '.$version, 'X-Requested-With: XMLHttpRequest']);
        $response['status'] === 302 || fail("anonymous GET /account returned HTTP {$response['status']}");
        ! str_contains($response['body'], 'worker-a@example.test') && ! str_contains($response['body'], 'worker-b@example.test') && ! str_contains($response['body'], 'acceptance@example.test') || fail('anonymous response exposed an authenticated identity');
    }

    printf("worker-isolation: account_requests=%d identities=3 anonymous=%d\n", $iterations * 4, $iterations);
}

match ($argv[1] ?? '') {
    'seed' => seed(),
    'warm' => isolation(100),
    'isolation' => isolation(2000),
    default => fail('usage: worker.php seed|isolation'),
};
