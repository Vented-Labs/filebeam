<?php

declare(strict_types=1);

namespace App\Models;

use Carbon\CarbonImmutable;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

/** @property CarbonImmutable $expires_at */
#[Fillable(['device_credential_id', 'idempotency_key_hash', 'replacement_token_hash', 'replacement_token_ciphertext', 'expires_at'])]
class DeviceCredentialRotation extends Model
{
    /** @return BelongsTo<DeviceCredential, $this> */
    public function credential(): BelongsTo
    {
        return $this->belongsTo(DeviceCredential::class, 'device_credential_id');
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return ['expires_at' => 'immutable_datetime'];
    }
}
