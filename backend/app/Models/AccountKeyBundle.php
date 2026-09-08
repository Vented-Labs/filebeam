<?php

declare(strict_types=1);

namespace App\Models;

use Database\Factories\AccountKeyBundleFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Hidden;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\HasMany;

#[Fillable(['user_id', 'version', 'public_key', 'fingerprint', 'custody_mode', 'encrypted_private_key', 'is_active', 'retired_at'])]
#[Hidden(['encrypted_private_key'])]
class AccountKeyBundle extends Model
{
    /** @use HasFactory<AccountKeyBundleFactory> */
    use HasFactory;

    /** @return BelongsTo<User, $this> */
    public function user(): BelongsTo
    {
        return $this->belongsTo(User::class);
    }

    /** @return HasMany<TransferKeyEnvelope, $this> */
    public function transferKeyEnvelopes(): HasMany
    {
        return $this->hasMany(TransferKeyEnvelope::class);
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function active(Builder $query): void
    {
        $query->where('is_active', true);
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'is_active' => 'boolean',
            'retired_at' => 'immutable_datetime',
        ];
    }
}
