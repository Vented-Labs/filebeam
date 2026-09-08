<?php

declare(strict_types=1);

namespace App\Models;

use Carbon\CarbonImmutable;
use Database\Factories\AdminAuditFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

/**
 * @property CarbonImmutable|null $created_at
 * @property CarbonImmutable|null $updated_at
 * @property array<string, mixed>|null $changes
 */
#[Fillable(['actor_id', 'action', 'target_type', 'target_id', 'reason', 'changes'])]
class AdminAudit extends Model
{
    /** @use HasFactory<AdminAuditFactory> */
    use HasFactory;

    /** @return BelongsTo<User, $this> */
    public function actor(): BelongsTo
    {
        return $this->belongsTo(User::class, 'actor_id');
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'changes' => 'array',
        ];
    }
}
