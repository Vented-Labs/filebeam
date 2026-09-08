<?php

declare(strict_types=1);

namespace App\Models;

use Database\Factories\TransferItemFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Concerns\HasUlids;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\HasMany;

#[Fillable([
    'id',
    'transfer_id',
    'position',
    'chunk_count',
    'declared_ciphertext_bytes',
    'ciphertext_bytes',
    'completed_at',
])]
class TransferItem extends Model
{
    /** @use HasFactory<TransferItemFactory> */
    use HasFactory, HasUlids;

    /** @return BelongsTo<Transfer, $this> */
    public function transfer(): BelongsTo
    {
        return $this->belongsTo(Transfer::class);
    }

    /** @return HasMany<TransferChunk, $this> */
    public function chunks(): HasMany
    {
        return $this->hasMany(TransferChunk::class)->orderBy('position');
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return ['completed_at' => 'immutable_datetime'];
    }
}
