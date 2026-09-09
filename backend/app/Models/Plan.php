<?php

declare(strict_types=1);

namespace App\Models;

use Database\Factories\PlanFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsToMany;
use Illuminate\Database\Eloquent\Relations\HasMany;

#[Fillable([
    'slug',
    'name',
    'maximum_transfer_bytes',
    'maximum_file_count',
    'maximum_note_bytes',
    'webrtc_maximum_transfer_bytes',
    'webrtc_maximum_file_count',
    'webrtc_maximum_note_bytes',
    'default_file_retention_hours',
    'maximum_file_retention_hours',
    'default_note_retention_hours',
    'maximum_note_retention_hours',
    'placement_mode',
    'is_active',
])]
class Plan extends Model
{
    /** @use HasFactory<PlanFactory> */
    use HasFactory;

    /** @return HasMany<Transfer, $this> */
    public function transfers(): HasMany
    {
        return $this->hasMany(Transfer::class);
    }

    /** @return HasMany<User, $this> */
    public function users(): HasMany
    {
        return $this->hasMany(User::class);
    }

    /** @return BelongsToMany<Filestore, $this> */
    public function filestores(): BelongsToMany
    {
        return $this->belongsToMany(Filestore::class, 'plan_filestore')->withPivot('is_default');
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function active(Builder $query): void
    {
        $query->where('is_active', true);
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function default(Builder $query, string $slug): void
    {
        $query->where('slug', $slug);
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'maximum_transfer_bytes' => 'integer',
            'maximum_file_count' => 'integer',
            'maximum_note_bytes' => 'integer',
            'webrtc_maximum_transfer_bytes' => 'integer',
            'webrtc_maximum_file_count' => 'integer',
            'webrtc_maximum_note_bytes' => 'integer',
            'default_file_retention_hours' => 'integer',
            'maximum_file_retention_hours' => 'integer',
            'default_note_retention_hours' => 'integer',
            'maximum_note_retention_hours' => 'integer',
            'is_active' => 'boolean',
        ];
    }
}
