<?php

declare(strict_types=1);

namespace App\Models;

use Database\Factories\FilestoreFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;

/** @property array<string, mixed>|null $configuration */
#[Fillable(['name', 'source', 'disk_name', 'driver', 'configuration', 'placement_enabled'])]
class Filestore extends Model
{
    /** @use HasFactory<FilestoreFactory> */
    use HasFactory;

    protected $hidden = ['configuration'];

    /** @return HasMany<TransferChunkLocation, $this> */
    public function locations(): HasMany
    {
        return $this->hasMany(TransferChunkLocation::class);
    }

    /** @return HasMany<TransferChunkUpload, $this> */
    public function uploadAttempts(): HasMany
    {
        return $this->hasMany(TransferChunkUpload::class);
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'configuration' => 'encrypted:array',
            'placement_enabled' => 'boolean',
            'physical_bytes' => 'integer',
            'attempt_bytes' => 'integer',
        ];
    }
}
