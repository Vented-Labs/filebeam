<?php

declare(strict_types=1);

namespace App\Services;

use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Http;

class FcmPushClient
{
    private const TOKEN_URL = 'https://oauth2.googleapis.com/token';

    private const SCOPE = 'https://www.googleapis.com/auth/firebase.messaging';

    /** @param array{project_id: string, client_email: string, private_key: string} $settings */
    public function send(array $settings, string $token, ?string $connectionId, int $registryRevision): bool
    {
        $accessToken = $this->accessToken($settings);
        if ($accessToken === null) {
            return false;
        }

        $response = Http::acceptJson()->timeout(10)->connectTimeout(5)->withToken($accessToken)->post(
            'https://fcm.googleapis.com/v1/projects/'.$settings['project_id'].'/messages:send',
            ['message' => ['token' => $token, 'data' => array_filter(['connection_id' => $connectionId, 'registry_revision' => (string) $registryRevision], static fn (?string $value): bool => $value !== null), 'android' => ['priority' => 'high']]],
        );

        return $response->successful();
    }

    /** @param array{project_id: string, client_email: string, private_key: string} $settings */
    private function accessToken(array $settings): ?string
    {
        $key = 'filebeam:push:fcm-token:'.hash('sha256', $settings['project_id']."\0".$settings['client_email']."\0".$settings['private_key']);
        $cached = Cache::get($key);
        if (is_string($cached) && $cached !== '') {
            return $cached;
        }

        $now = now()->getTimestamp();
        $assertion = $this->jwt(['alg' => 'RS256', 'typ' => 'JWT'], [
            'iss' => $settings['client_email'], 'scope' => self::SCOPE, 'aud' => self::TOKEN_URL, 'iat' => $now, 'exp' => $now + 3600,
        ], $settings['private_key'], OPENSSL_ALGO_SHA256);
        if ($assertion === null) {
            return null;
        }
        $response = Http::asForm()->acceptJson()->timeout(10)->connectTimeout(5)->post(self::TOKEN_URL, [
            'grant_type' => 'urn:ietf:params:oauth:grant-type:jwt-bearer', 'assertion' => $assertion,
        ]);
        $token = $response->json('access_token');
        $expiresIn = $response->json('expires_in');
        if (! $response->successful() || ! is_string($token) || $token === '' || ! is_int($expiresIn) && ! ctype_digit((string) $expiresIn)) {
            return null;
        }
        Cache::put($key, $token, now()->addSeconds(max(1, (int) $expiresIn - 60)));

        return $token;
    }

    /**
     * @param  array<string, mixed>  $header
     * @param  array<string, mixed>  $claims
     */
    private function jwt(array $header, array $claims, string $privateKey, int $algorithm): ?string
    {
        $input = $this->encode($header).'.'.$this->encode($claims);
        $signature = '';
        if (! extension_loaded('openssl') || openssl_sign($input, $signature, $privateKey, $algorithm) !== true) {
            return null;
        }

        return $input.'.'.rtrim(strtr(base64_encode($signature), '+/', '-_'), '=');
    }

    /** @param array<string, mixed> $value */
    private function encode(array $value): string
    {
        return rtrim(strtr(base64_encode(json_encode($value, JSON_THROW_ON_ERROR)), '+/', '-_'), '=');
    }
}
