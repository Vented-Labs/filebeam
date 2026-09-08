<?php

declare(strict_types=1);

namespace App\Models;

use Carbon\CarbonImmutable;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Model;

/**
 * Browser resumable-upload state. This deliberately does not share replica-attempt rows.
 *
 * @property CarbonImmutable $expires_at
 */
#[Fillable(['id', 'transfer_id', 'transfer_item_id', 'position', 'ciphertext_bytes', 'checksum', 'offset', 'parts', 'state', 'expires_at', 'released_at'])]
class TransferChunkStage extends Model
{
    public $incrementing = false;

    protected $keyType = 'string';

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'position' => 'integer',
            'ciphertext_bytes' => 'integer',
            'offset' => 'integer',
            'parts' => 'array',
            'expires_at' => 'immutable_datetime',
            'released_at' => 'immutable_datetime',
        ];
    }
}
