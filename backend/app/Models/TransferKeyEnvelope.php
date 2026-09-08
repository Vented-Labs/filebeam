<?php

declare(strict_types=1);

namespace App\Models;

use Database\Factories\TransferKeyEnvelopeFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

#[Fillable(['transfer_id', 'account_key_bundle_id', 'role', 'encrypted_key'])]
class TransferKeyEnvelope extends Model
{
    /** @use HasFactory<TransferKeyEnvelopeFactory> */
    use HasFactory;

    /** @return BelongsTo<Transfer, $this> */
    public function transfer(): BelongsTo
    {
        return $this->belongsTo(Transfer::class);
    }

    /** @return BelongsTo<AccountKeyBundle, $this> */
    public function accountKeyBundle(): BelongsTo
    {
        return $this->belongsTo(AccountKeyBundle::class);
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function recipient(Builder $query): void
    {
        $query->where('role', 'recipient');
    }
}
