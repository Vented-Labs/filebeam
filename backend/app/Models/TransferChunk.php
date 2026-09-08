<?php

declare(strict_types=1);

namespace App\Models;

use Database\Factories\TransferChunkFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\HasMany;

#[Fillable(['transfer_item_id', 'position', 'ciphertext_bytes', 'checksum'])]
class TransferChunk extends Model
{
    /** @use HasFactory<TransferChunkFactory> */
    use HasFactory;

    /** @return HasMany<TransferChunkLocation, $this> */
    public function locations(): HasMany
    {
        return $this->hasMany(TransferChunkLocation::class);
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function atPosition(Builder $query, int $position): void
    {
        $query->where('position', $position);
    }

    /** @return BelongsTo<TransferItem, $this> */
    public function item(): BelongsTo
    {
        return $this->belongsTo(TransferItem::class, 'transfer_item_id');
    }
}
