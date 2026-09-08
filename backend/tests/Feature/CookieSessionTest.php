<?php

declare(strict_types=1);

use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;
use Symfony\Component\HttpFoundation\Cookie;

test('the session driver defaults to cookie and accepts an environment override', function (): void {
    $serverValue = $_SERVER['SESSION_DRIVER'] ?? null;
    $environmentValue = $_ENV['SESSION_DRIVER'] ?? null;
    $getenvValue = getenv('SESSION_DRIVER');
    $serverHasValue = array_key_exists('SESSION_DRIVER', $_SERVER);
    $environmentHasValue = array_key_exists('SESSION_DRIVER', $_ENV);

    try {
        unset($_SERVER['SESSION_DRIVER'], $_ENV['SESSION_DRIVER']);
        putenv('SESSION_DRIVER');

        expect((require config_path('session.php'))['driver'])->toBe('cookie');

        $_SERVER['SESSION_DRIVER'] = 'array';
        $_ENV['SESSION_DRIVER'] = 'array';
        putenv('SESSION_DRIVER=array');

        expect((require config_path('session.php'))['driver'])->toBe('array');
    } finally {
        if ($serverHasValue) {
            $_SERVER['SESSION_DRIVER'] = $serverValue;
        } else {
            unset($_SERVER['SESSION_DRIVER']);
        }

        if ($environmentHasValue) {
            $_ENV['SESSION_DRIVER'] = $environmentValue;
        } else {
            unset($_ENV['SESSION_DRIVER']);
        }

        if ($getenvValue === false) {
            putenv('SESSION_DRIVER');
        } else {
            putenv("SESSION_DRIVER={$getenvValue}");
        }
    }
});

test('cookie sessions persist only through the encrypted response cookie', function (): void {
    config()->set('session.driver', 'cookie');
    config()->set('session.cookie', 'cookie-session-test');
    app('session')->forgetDrivers();

    Route::middleware('web')->get('/__tests/cookie-session', function (Request $request) {
        if ($request->query->has('value')) {
            $request->session()->put('value', $request->query('value'));
        }

        return response()->json(['value' => $request->session()->get('value')]);
    });

    $firstResponse = $this->get('/__tests/cookie-session?value=from-cookie')
        ->assertOk()
        ->assertJsonPath('value', 'from-cookie');
    $responseCookies = collect($firstResponse->headers->getCookies())
        ->mapWithKeys(fn (Cookie $cookie): array => [$cookie->getName() => $cookie->getValue()])
        ->all();
    $payloadCookie = collect($firstResponse->headers->getCookies())->first(
        fn (Cookie $cookie): bool => $cookie->getName() !== config('session.cookie')
            && str_contains(app('encrypter')->decrypt($cookie->getValue(), false), 'from-cookie'),
    );

    expect($responseCookies)->toHaveKey(config('session.cookie'))
        ->and($payloadCookie)->toBeInstanceOf(Cookie::class)
        ->and(app('encrypter')->decrypt($payloadCookie->getValue(), false))->toContain('from-cookie');

    app('session')->flush();
    app('session')->forgetDrivers();

    $this->get('/__tests/cookie-session')
        ->assertOk()
        ->assertJsonPath('value', null);

    app('session')->flush();
    app('session')->forgetDrivers();

    $this->withUnencryptedCookies($responseCookies)
        ->get('/__tests/cookie-session')
        ->assertOk()
        ->assertJsonPath('value', 'from-cookie');
});
