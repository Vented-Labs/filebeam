<?php

declare(strict_types=1);

namespace App\Models;

use Carbon\CarbonImmutable;
use Database\Factories\TransferChunkUploadFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Concerns\HasUlids;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

/**
 * @property CarbonImmutable $valid_until
 * @property CarbonImmutable|null $cleanup_started_at
 * @property bool $is_reaping
 */
#[Fillable(['id', 'transfer_id', 'transfer_item_id', 'position', 'filestore_id', 'storage_path', 'ciphertext_bytes', 'checksum', 'valid_until', 'is_reaping', 'cleanup_started_at'])]
class TransferChunkUpload extends Model
{
    /** @use HasFactory<TransferChunkUploadFactory> */
    use HasFactory, HasUlids;

    /** @return BelongsTo<Filestore, $this> */
    public function filestore(): BelongsTo
    {
        return $this->belongsTo(Filestore::class);
    }

    /** @return BelongsTo<TransferItem, $this> */
    public function item(): BelongsTo
    {
        return $this->belongsTo(TransferItem::class, 'transfer_item_id');
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function atPosition(Builder $query, int $position): void
    {
        $query->where('position', $position);
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'valid_until' => 'immutable_datetime',
            'is_reaping' => 'boolean',
            'cleanup_started_at' => 'immutable_datetime',
        ];
    }
}
