<?php

declare(strict_types=1);

namespace App\Models;

use App\Enums\DeviceStatus;
use Carbon\CarbonImmutable;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Casts\Attribute;
use Illuminate\Database\Eloquent\Concerns\HasUlids;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\HasMany;

/**
 * @property string $id
 * @property int $user_id
 * @property DeviceStatus $status
 * @property int $generation
 * @property non-empty-string $public_key
 * @property int $key_version
 * @property CarbonImmutable|null $enrollment_expires_at
 * @property CarbonImmutable|null $approved_at
 * @property CarbonImmutable|null $credential_issued_at
 * @property CarbonImmutable|null $last_seen_at
 * @property CarbonImmutable|null $presence_expires_at
 * @property CarbonImmutable|null $revoked_at
 */
#[Fillable(['id', 'user_id', 'display_name', 'platform', 'public_key', 'key_version', 'status', 'generation', 'capabilities', 'enrollment_challenge_hash', 'enrollment_expires_at', 'approval_signature', 'approved_by_device_id', 'approved_at', 'credential_issued_at', 'last_seen_at', 'presence_expires_at', 'revoked_at'])]
class Device extends Model
{
    use HasUlids;

    /** @return BelongsTo<User, $this> */
    public function user(): BelongsTo
    {
        return $this->belongsTo(User::class);
    }

    /** @return HasMany<DeviceCredential, $this> */
    public function credentials(): HasMany
    {
        return $this->hasMany(DeviceCredential::class);
    }

    /** @return Attribute<non-empty-string, non-empty-string> */
    protected function publicKey(): Attribute
    {
        return Attribute::make(
            get: fn (string $value): string => base64_decode($value, true) ?: throw new \LogicException('Invalid stored device public key.'),
            set: fn (string $value): string => base64_encode($value),
        );
    }

    /** @return Attribute<string|null, string|null> */
    protected function approvalSignature(): Attribute
    {
        return Attribute::make(
            get: fn (?string $value): ?string => $value === null ? null : (base64_decode($value, true) ?: throw new \LogicException('Invalid stored device approval signature.')),
            set: fn (?string $value): ?string => $value === null ? null : base64_encode($value),
        );
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function active(Builder $query): void
    {
        $query->where('status', DeviceStatus::Active);
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'status' => DeviceStatus::class,
            'generation' => 'integer',
            'capabilities' => 'array',
            'enrollment_expires_at' => 'immutable_datetime',
            'approved_at' => 'immutable_datetime',
            'credential_issued_at' => 'immutable_datetime',
            'last_seen_at' => 'immutable_datetime',
            'presence_expires_at' => 'immutable_datetime',
            'revoked_at' => 'immutable_datetime',
        ];
    }
}
