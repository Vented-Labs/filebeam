<?php

declare(strict_types=1);

namespace App\Models;

use Database\Factories\TransferChunkLocationFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

#[Fillable(['transfer_chunk_id', 'filestore_id', 'storage_path', 'ciphertext_bytes'])]
class TransferChunkLocation extends Model
{
    /** @use HasFactory<TransferChunkLocationFactory> */
    use HasFactory;

    /** @return BelongsTo<TransferChunk, $this> */
    public function chunk(): BelongsTo
    {
        return $this->belongsTo(TransferChunk::class, 'transfer_chunk_id');
    }

    /** @return BelongsTo<Filestore, $this> */
    public function filestore(): BelongsTo
    {
        return $this->belongsTo(Filestore::class);
    }
}
