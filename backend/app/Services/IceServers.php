<?php

declare(strict_types=1);

namespace App\Services;

use Illuminate\Support\Str;

class IceServers
{
    /** @return list<array<string, mixed>> */
    public function forConnection(): array
    {
        $servers = config('filebeam.webrtc.ice_servers', []);
        $servers = is_array($servers) ? array_values($servers) : [];
        $urls = config('filebeam.webrtc.turn_urls', []);
        $secret = config('filebeam.webrtc.turn_secret');
        if (! is_array($urls) || $urls === [] || ! is_string($secret) || $secret === '') {
            return $servers;
        }
        $username = (string) now()->addSeconds((int) config('filebeam.webrtc.turn_ttl_seconds', 3600))->getTimestamp().':'.Str::random(16);
        $servers[] = ['urls' => array_values($urls), 'username' => $username, 'credential' => base64_encode(hash_hmac('sha1', $username, $secret, true))];

        return $servers;
    }
}
