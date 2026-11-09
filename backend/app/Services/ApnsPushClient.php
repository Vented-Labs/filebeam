<?php

declare(strict_types=1);

namespace App\Services;

use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Http;

class ApnsPushClient
{
    /** @param array{team_id: string, key_id: string, bundle_id: string, private_key: string, sandbox: bool} $settings */
    public function send(array $settings, string $token, ?string $connectionId, int $registryRevision): bool
    {
        $jwt = $this->jwt($settings);
        if ($jwt === null) {
            return false;
        }
        $host = $settings['sandbox'] ? 'https://api.sandbox.push.apple.com' : 'https://api.push.apple.com';
        $response = Http::acceptJson()->timeout(10)->connectTimeout(5)->withHeaders([
            'authorization' => 'bearer '.$jwt,
            'apns-topic' => $settings['bundle_id'],
            'apns-push-type' => 'background',
            'apns-priority' => '5',
        ])->post($host.'/3/device/'.$token, array_filter([
            'aps' => ['content-available' => 1], 'connection_id' => $connectionId, 'registry_revision' => $registryRevision,
        ], static fn (mixed $value): bool => $value !== null));

        return $response->successful();
    }

    /** @param array{team_id: string, key_id: string, bundle_id: string, private_key: string, sandbox: bool} $settings */
    private function jwt(array $settings): ?string
    {
        $key = 'filebeam:push:apns-jwt:'.hash('sha256', $settings['team_id']."\0".$settings['key_id']."\0".$settings['private_key']);
        $cached = Cache::get($key);
        if (is_string($cached) && $cached !== '') {
            return $cached;
        }
        $input = $this->encode(['alg' => 'ES256', 'kid' => $settings['key_id']]).'.'.$this->encode(['iss' => $settings['team_id'], 'iat' => now()->getTimestamp()]);
        $der = '';
        if (! extension_loaded('openssl') || openssl_sign($input, $der, $settings['private_key'], OPENSSL_ALGO_SHA256) !== true) {
            return null;
        }
        $signature = $this->joseSignature($der);
        if ($signature === null) {
            return null;
        }
        $jwt = $input.'.'.rtrim(strtr(base64_encode($signature), '+/', '-_'), '=');
        // APNs rejects provider tokens older than one hour. Refresh well before that limit.
        Cache::put($key, $jwt, now()->addMinutes(50));

        return $jwt;
    }

    /** @param array<string, mixed> $value */
    private function encode(array $value): string
    {
        return rtrim(strtr(base64_encode(json_encode($value, JSON_THROW_ON_ERROR)), '+/', '-_'), '=');
    }

    private function joseSignature(string $der): ?string
    {
        $offset = 0;
        if (($der[$offset++] ?? null) !== "\x30") {
            return null;
        }
        $sequenceLength = $this->length($der, $offset);
        if ($sequenceLength === null || $sequenceLength !== strlen($der) - $offset) {
            return null;
        }
        if (($der[$offset++] ?? null) !== "\x02") {
            return null;
        }
        $r = $this->integer($der, $offset);
        if (($der[$offset++] ?? null) !== "\x02") {
            return null;
        }
        $s = $this->integer($der, $offset);
        if ($r === null || $s === null || $offset !== strlen($der)) {
            return null;
        }

        return str_pad(ltrim($r, "\0"), 32, "\0", STR_PAD_LEFT).str_pad(ltrim($s, "\0"), 32, "\0", STR_PAD_LEFT);
    }

    private function integer(string $der, int &$offset): ?string
    {
        $length = $this->length($der, $offset);
        if ($length === null || $length < 1 || $offset + $length > strlen($der)) {
            return null;
        }

        $integer = substr($der, $offset, $length);
        $offset += $length;

        return $integer;
    }

    private function length(string $der, int &$offset): ?int
    {
        $first = ord($der[$offset++] ?? "\0");
        if ($first < 128) {
            return $first;
        }
        $bytes = $first & 0x7F;
        if ($bytes < 1 || $bytes > 2 || $offset + $bytes > strlen($der)) {
            return null;
        }
        $length = 0;
        for ($index = 0; $index < $bytes; $index++) {
            $length = ($length << 8) | ord($der[$offset++]);
        }

        return $length;
    }
}
