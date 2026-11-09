<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use App\Enums\DeviceStatus;
use App\Models\DeviceCredential;
use App\Models\DeviceCredentialRotation;
use Closure;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

class AuthenticateDeviceCredential
{
    public function handle(Request $request, Closure $next): Response
    {
        $token = $request->bearerToken();
        abort_unless(is_string($token) && strlen($token) === 64, 401);
        $credential = DeviceCredential::query()->with('device.user')->where('token_hash', hash('sha256', $token))->first();
        $replayingRotation = $credential instanceof DeviceCredential && $request->is('api/native/v1/devices/credentials/rotate') && is_string($request->header('Idempotency-Key')) && DeviceCredentialRotation::query()->where('device_credential_id', $credential->id)->where('idempotency_key_hash', hash('sha256', $request->header('Idempotency-Key')))->where('expires_at', '>', now())->exists();
        abort_unless($credential instanceof DeviceCredential && ($credential->revoked_at === null || $replayingRotation) && $credential->expires_at->isFuture(), 401);
        $device = $credential->device;
        abort_unless($device !== null && $device->status === DeviceStatus::Active && $device->generation === $device->user->device_generation, 403);
        // Daemons poll every few seconds; record use at most once a minute.
        if ($credential->last_used_at === null || $credential->last_used_at->lt(now()->subMinute())) {
            $credential->update(['last_used_at' => now()]);
        }
        $request->attributes->set('device', $device);
        $request->attributes->set('device_credential', $credential);

        return $next($request);
    }
}
