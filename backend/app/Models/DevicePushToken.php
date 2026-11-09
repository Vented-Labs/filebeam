<?php

declare(strict_types=1);

namespace App\Models;

use Carbon\CarbonImmutable;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

/**
 * @property int $id
 * @property string $device_id
 * @property string $provider
 * @property string $token_hash
 * @property string $token_ciphertext
 * @property CarbonImmutable|null $revoked_at
 */
#[Fillable(['device_id', 'provider', 'token_hash', 'token_ciphertext', 'revoked_at'])]
class DevicePushToken extends Model
{
    /** @return BelongsTo<Device, $this> */
    public function device(): BelongsTo
    {
        return $this->belongsTo(Device::class);
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function current(Builder $query): void
    {
        $query->whereNull('revoked_at');
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return ['revoked_at' => 'immutable_datetime'];
    }
}
