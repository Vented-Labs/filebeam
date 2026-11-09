<?php

declare(strict_types=1);

namespace App\Services;

use App\Enums\DeviceStatus;
use App\Models\Device;
use App\Models\DevicePushToken;
use Illuminate\Database\QueryException;
use Illuminate\Support\Facades\Crypt;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\RateLimiter;

class PushWakeService
{
    public function __construct(private readonly PushSettings $settings, private readonly ApnsPushClient $apns, private readonly FcmPushClient $fcm) {}

    /**
     * Handoff API for the device controller: persist a provider token only for an active device.
     */
    public function register(Device $device, string $provider, string $token): void
    {
        if (! in_array($provider, ['apns', 'fcm'], true) || preg_match('/\A[^\x00-\x1f\x7f]{1,4096}\z/', $token) !== 1) {
            abort(422, 'Invalid push token.');
        }
        $hash = hash('sha256', $token);

        for ($attempt = 0; $attempt < 3; $attempt++) {
            try {
                DB::transaction(function () use ($device, $provider, $token, $hash): void {
                    $current = Device::query()->lockForUpdate()->findOrFail($device->id);
                    abort_unless($current->status === DeviceStatus::Active, 403);
                    $record = DevicePushToken::query()->where('provider', $provider)->where('token_hash', $hash)->lockForUpdate()->first();
                    if ($record === null) {
                        DevicePushToken::query()->create(['device_id' => $current->id, 'provider' => $provider, 'token_hash' => $hash, 'token_ciphertext' => Crypt::encryptString($token)]);

                        return;
                    }
                    $record->update(['device_id' => $current->id, 'token_ciphertext' => Crypt::encryptString($token), 'revoked_at' => null]);
                }, 3);

                return;
            } catch (QueryException $exception) {
                if ($attempt === 2) {
                    throw $exception;
                }
            }
        }
    }

    /**
     * Handoff API for connection creation. A returned false is never a transfer or connection success signal.
     */
    public function wake(Device $device, string $connectionId): bool
    {
        if (! filter_var(config('filebeam.push.enabled'), FILTER_VALIDATE_BOOLEAN) || preg_match('/\A[0-9A-HJKMNP-TV-Z]{26}\z/', $connectionId) !== 1) {
            return false;
        }

        return $this->send($device, $connectionId);
    }

    /** Wake a current device to refresh its registry after another device is revoked. */
    public function wakeRegistry(Device $device): bool
    {
        if (! filter_var(config('filebeam.push.enabled'), FILTER_VALIDATE_BOOLEAN)) {
            return false;
        }

        return $this->send($device, null);
    }

    private function send(Device $device, ?string $connectionId): bool
    {
        $settings = $this->settings->current();
        if (! $settings['enabled']) {
            return false;
        }
        $current = Device::query()->with('user')->find($device->id);
        if ($current === null || $current->status !== DeviceStatus::Active) {
            return false;
        }
        $limitKey = 'filebeam:push:wake:'.$current->id;
        if (RateLimiter::tooManyAttempts($limitKey, (int) config('filebeam.push.wakes_per_minute'))) {
            return false;
        }
        RateLimiter::hit($limitKey, 60);
        $sent = false;
        $revision = $current->user->device_registry_revision;
        foreach (DevicePushToken::query()->where('device_id', $current->id)->current()->get() as $registration) {
            try {
                $token = Crypt::decryptString($registration->token_ciphertext);
                $delivered = match ($registration->provider) {
                    'apns' => $settings['apns'] === null ? false : $this->apns->send($settings['apns'], $token, $connectionId, $revision),
                    'fcm' => $settings['fcm'] === null ? false : $this->fcm->send($settings['fcm'], $token, $connectionId, $revision),
                    default => false,
                };
                $sent = $sent || $delivered;
            } catch (\Throwable) {
                // Provider and decrypt failures are intentionally opaque to logs and callers.
            }
        }

        return $sent;
    }

    /** Handoff API for device revocation and reset. */
    public function revoke(Device $device): void
    {
        DevicePushToken::query()->where('device_id', $device->id)->current()->update(['revoked_at' => now()]);
    }
}
