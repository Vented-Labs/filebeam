<?php

declare(strict_types=1);

namespace App\Models;

use App\Enums\TransferDelivery;
use App\Enums\TransferKind;
use App\Enums\TransferStatus;
use Carbon\CarbonImmutable;
use Database\Factories\TransferFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Hidden;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Concerns\HasUlids;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\HasMany;
use Illuminate\Database\Eloquent\Relations\HasManyThrough;

/**
 * @property TransferDelivery $delivery
 * @property TransferKind $kind
 * @property TransferStatus $status
 * @property CarbonImmutable|null $completed_at
 * @property string|null $encrypted_descriptor
 * @property CarbonImmutable $expires_at
 * @property list<int>|null $filestore_ids
 */
#[Fillable([
    'id',
    'kind',
    'delivery',
    'owner_id',
    'recipient_id',
    'plan_id',
    'filestore_ids',
    'placement_mode',
    'status',
    'protocol_version',
    'chunk_bytes',
    'retention_hours',
    'burn_on_read',
    'encrypted_manifest',
    'encrypted_descriptor',
    'declared_ciphertext_bytes',
    'ciphertext_bytes',
    'item_count',
    'upload_token_hash',
    'delete_token_hash',
    'read_token_hash',
    'monitor_token_hash',
    'completed_at',
    'expires_at',
])]
#[Hidden(['upload_token_hash', 'delete_token_hash', 'read_token_hash', 'monitor_token_hash'])]
class Transfer extends Model
{
    /** @use HasFactory<TransferFactory> */
    use HasFactory, HasUlids;

    /** @return BelongsTo<User, $this> */
    public function owner(): BelongsTo
    {
        return $this->belongsTo(User::class, 'owner_id');
    }

    /** @return BelongsTo<User, $this> */
    public function recipient(): BelongsTo
    {
        return $this->belongsTo(User::class, 'recipient_id');
    }

    /** @return BelongsTo<Plan, $this> */
    public function plan(): BelongsTo
    {
        return $this->belongsTo(Plan::class);
    }

    /** @return HasMany<TransferItem, $this> */
    public function items(): HasMany
    {
        return $this->hasMany(TransferItem::class)->orderBy('position');
    }

    /** @return HasManyThrough<TransferChunk, TransferItem, $this> */
    public function chunks(): HasManyThrough
    {
        return $this->hasManyThrough(TransferChunk::class, TransferItem::class);
    }

    public function isPublishedTurbo(): bool
    {
        return $this->kind === TransferKind::Files
            && $this->delivery === TransferDelivery::Link
            && $this->protocol_version === 1
            && is_string($this->encrypted_descriptor)
            && in_array($this->status, [TransferStatus::Pending, TransferStatus::Available], true);
    }

    /** @return HasMany<FileReport, $this> */
    public function reports(): HasMany
    {
        return $this->hasMany(FileReport::class)->orderByDesc('created_at');
    }

    /** @return HasMany<AdminAudit, $this> */
    public function moderationActivity(): HasMany
    {
        return $this->hasMany(AdminAudit::class, 'target_id')
            ->where('target_type', self::class)
            ->orderByDesc('created_at');
    }

    /** @return HasMany<TransferKeyEnvelope, $this> */
    public function keyEnvelopes(): HasMany
    {
        return $this->hasMany(TransferKeyEnvelope::class);
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function pending(Builder $query): void
    {
        $query->where('status', TransferStatus::Pending);
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function availableAndUnexpired(Builder $query): void
    {
        $query->where('status', TransferStatus::Available)
            ->where('expires_at', '>', now());
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function awaitingCleanup(Builder $query): void
    {
        $query->where('status', TransferStatus::Deleting);
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'filestore_ids' => 'array',
            'kind' => TransferKind::class,
            'delivery' => TransferDelivery::class,
            'status' => TransferStatus::class,
            'burn_on_read' => 'boolean',
            'completed_at' => 'immutable_datetime',
            'expires_at' => 'immutable_datetime',
        ];
    }
}
