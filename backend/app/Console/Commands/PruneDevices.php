<?php

declare(strict_types=1);

namespace App\Console\Commands;

use App\Enums\DeviceStatus;
use App\Models\Device;
use App\Models\DeviceCredential;
use App\Models\DeviceCredentialRotation;
use App\Models\DevicePushToken;
use Illuminate\Console\Attributes\Description;
use Illuminate\Console\Attributes\Signature;
use Illuminate\Console\Command;
use Illuminate\Support\Facades\DB;

#[Signature('filebeam:prune-devices')]
#[Description('Remove expired device enrollment and credential replay state')]
class PruneDevices extends Command
{
    public function handle(): int
    {
        $now = now();
        Device::query()->where('status', DeviceStatus::Pending)->where('enrollment_expires_at', '<=', $now)->select('id')->chunkById(100, function ($devices): void {
            Device::query()->whereKey($devices->pluck('id'))->where('status', DeviceStatus::Pending)->delete();
        });
        Device::query()->whereNotNull('presence_expires_at')->where('presence_expires_at', '<=', $now)->update(['presence_expires_at' => null]);
        DeviceCredential::query()->where(function ($query) use ($now): void {
            $query->where('expires_at', '<=', $now)->orWhere(function ($revoked) use ($now): void {
                $revoked->whereNotNull('revoked_at')->where('updated_at', '<=', $now->copy()->subDay());
            });
        })->select('id')->chunkById(100, function ($credentials): void {
            DeviceCredential::query()->whereKey($credentials->pluck('id'))->delete();
        });
        DeviceCredentialRotation::query()->where('expires_at', '<=', $now)->select('id')->chunkById(100, function ($rotations): void {
            DB::table('device_credential_rotations')->whereIn('id', $rotations->pluck('id'))->delete();
        });
        DevicePushToken::query()->whereNotNull('revoked_at')->where('revoked_at', '<=', $now->copy()->subDay())->select('id')->chunkById(100, function ($tokens): void {
            DevicePushToken::query()->whereKey($tokens->pluck('id'))->delete();
        });

        return self::SUCCESS;
    }
}
