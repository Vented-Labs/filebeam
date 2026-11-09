<?php

declare(strict_types=1);

namespace App\Services;

use App\Enums\DeviceStatus;
use App\Models\Device;
use App\Models\DeviceCredential;
use App\Models\DeviceCredentialRotation;
use App\Models\DevicePushToken;
use App\Models\User;
use App\Support\DevicePolicy;
use Illuminate\Support\Facades\Crypt;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Str;

class DeviceRegistry
{
    public function activate(Device $device, ?Device $approver = null, ?string $approvalSignature = null): Device
    {
        return DB::transaction(function () use ($device, $approver, $approvalSignature): Device {
            $locked = Device::query()->lockForUpdate()->findOrFail($device->id);
            abort_unless($locked->status === DeviceStatus::Pending && $locked->enrollment_expires_at?->isFuture(), 409);
            $user = User::query()->lockForUpdate()->findOrFail($locked->user_id);
            abort_unless($locked->generation === $user->device_generation, 409);
            if ($approver === null) {
                abort_unless(! Device::query()->where('user_id', $user->id)->where('generation', $user->device_generation)->active()->whereKeyNot($locked->id)->exists(), 409);
            }
            abort_unless(Device::query()->where('user_id', $user->id)->active()->whereKeyNot($locked->id)->count() < app(DevicePolicy::class)->activeLimit(), 429, 'Active device limit reached.');
            $locked->update([
                'status' => DeviceStatus::Active,
                'approved_by_device_id' => $approver?->id,
                'approval_signature' => $approvalSignature,
                'approved_at' => now(),
            ]);
            $user->increment('device_registry_revision');

            return $locked->refresh();
        }, 3);
    }

    public function issueCredential(Device $device): string
    {
        return DB::transaction(function () use ($device): string {
            $locked = Device::query()->lockForUpdate()->findOrFail($device->id);
            abort_unless($locked->status === DeviceStatus::Active && $locked->credential_issued_at === null && $locked->enrollment_expires_at?->isFuture(), 409);
            $token = Str::random(64);
            DeviceCredential::query()->create(['device_id' => $locked->id, 'token_hash' => hash('sha256', $token), 'expires_at' => now()->addDays(90)]);
            $locked->update(['credential_issued_at' => now(), 'enrollment_challenge_hash' => null, 'enrollment_expires_at' => null]);

            return $token;
        }, 3);
    }

    public function revoke(User $actor, Device $device): Device
    {
        return DB::transaction(function () use ($actor, $device): Device {
            $user = User::query()->lockForUpdate()->findOrFail($actor->id);
            $locked = Device::query()->lockForUpdate()->where('user_id', $user->id)->findOrFail($device->id);
            if ($locked->status !== DeviceStatus::Revoked) {
                $locked->update(['status' => DeviceStatus::Revoked, 'revoked_at' => now(), 'presence_expires_at' => now()]);
                DeviceCredential::query()->where('device_id', $locked->id)->whereNull('revoked_at')->update(['revoked_at' => now()]);
                DevicePushToken::query()->where('device_id', $locked->id)->current()->update(['revoked_at' => now()]);
                $user->increment('device_registry_revision');
            }

            return $locked->refresh();
        }, 3);
    }

    public function reset(User $actor): User
    {
        return DB::transaction(function () use ($actor): User {
            $user = User::query()->lockForUpdate()->findOrFail($actor->id);
            $devices = Device::query()->where('user_id', $user->id)->lockForUpdate()->get();
            foreach ($devices as $device) {
                if ($device->status !== DeviceStatus::Revoked) {
                    $device->update(['status' => DeviceStatus::Revoked, 'revoked_at' => now(), 'presence_expires_at' => now()]);
                }
            }
            DeviceCredential::query()->whereIn('device_id', $devices->modelKeys())->whereNull('revoked_at')->update(['revoked_at' => now()]);
            DevicePushToken::query()->whereIn('device_id', $devices->modelKeys())->current()->update(['revoked_at' => now()]);
            $user->increment('device_generation');
            $user->increment('device_registry_revision');

            return $user->refresh();
        }, 3);
    }

    public function rotateCredential(DeviceCredential $credential, string $idempotencyKey): string
    {
        $device = Device::query()->findOrFail($credential->device_id);

        return DB::transaction(function () use ($credential, $device, $idempotencyKey): string {
            $user = User::query()->lockForUpdate()->findOrFail($device->user_id);
            $lockedDevice = Device::query()->where('user_id', $user->id)->lockForUpdate()->findOrFail($device->id);
            $locked = DeviceCredential::query()->where('device_id', $lockedDevice->id)->lockForUpdate()->findOrFail($credential->id);
            $keyHash = hash('sha256', $idempotencyKey);
            $existing = DeviceCredentialRotation::query()->where('idempotency_key_hash', $keyHash)->lockForUpdate()->first();
            if ($existing !== null) {
                abort_unless($existing->device_credential_id === $locked->id && $existing->expires_at->isFuture(), 409);

                return Crypt::decryptString($existing->replacement_token_ciphertext);
            }
            abort_unless($locked->revoked_at === null && $locked->expires_at->isFuture() && $lockedDevice->status === DeviceStatus::Active && $lockedDevice->generation === $user->device_generation, 403);
            $token = Str::random(64);
            DeviceCredential::query()->create(['device_id' => $locked->device_id, 'token_hash' => hash('sha256', $token), 'expires_at' => now()->addDays(90)]);
            $locked->update(['revoked_at' => now()]);
            DeviceCredentialRotation::query()->create(['device_credential_id' => $locked->id, 'idempotency_key_hash' => $keyHash, 'replacement_token_hash' => hash('sha256', $token), 'replacement_token_ciphertext' => Crypt::encryptString($token), 'expires_at' => now()->addDay()]);

            return $token;
        }, 3);
    }
}
