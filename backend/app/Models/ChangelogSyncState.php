<?php

declare(strict_types=1);

namespace App\Models;

use Carbon\CarbonImmutable;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Model;

/**
 * @property string $source
 * @property int $generation
 * @property CarbonImmutable|null $last_attempt_at
 * @property CarbonImmutable|null $last_success_at
 * @property CarbonImmutable|null $last_full_success_at
 * @property int|null $latest_upstream_id
 * @property CarbonImmutable|null $next_attempt_at
 * @property int $failures
 * @property string|null $last_error
 * @property array<string, array{etag: string, ids: list<int>, next: bool}>|null $pages
 */
#[Fillable(['source', 'generation', 'last_attempt_at', 'last_success_at', 'last_full_success_at', 'latest_upstream_id', 'next_attempt_at', 'failures', 'last_error', 'pages'])]
class ChangelogSyncState extends Model
{
    protected $primaryKey = 'source';

    protected $keyType = 'string';

    public $incrementing = false;

    public static function for(string $source): self
    {
        return self::query()->firstOrCreate(['source' => $source], ['generation' => 0, 'failures' => 0]);
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'generation' => 'integer',
            'last_attempt_at' => 'immutable_datetime',
            'last_success_at' => 'immutable_datetime',
            'last_full_success_at' => 'immutable_datetime',
            'latest_upstream_id' => 'integer',
            'next_attempt_at' => 'immutable_datetime',
            'failures' => 'integer',
            'pages' => 'array',
        ];
    }
}
