<?php

declare(strict_types=1);

namespace App\Models;

use App\Enums\ReportStatus;
use Carbon\CarbonImmutable;
use Database\Factories\FileReportFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Concerns\HasUlids;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\HasMany;

/**
 * @property ReportStatus $status
 * @property CarbonImmutable|null $resolved_at
 */
#[Fillable([
    'transfer_id',
    'transfer_identifier',
    'reporter_id',
    'reporter_email',
    'category',
    'description',
    'status',
    'assigned_to',
    'resolution',
    'resolved_at',
])]
class FileReport extends Model
{
    /** @use HasFactory<FileReportFactory> */
    use HasFactory, HasUlids;

    /** @return BelongsTo<Transfer, $this> */
    public function transfer(): BelongsTo
    {
        return $this->belongsTo(Transfer::class);
    }

    /** @return BelongsTo<User, $this> */
    public function reporter(): BelongsTo
    {
        return $this->belongsTo(User::class, 'reporter_id');
    }

    /** @return BelongsTo<User, $this> */
    public function assignee(): BelongsTo
    {
        return $this->belongsTo(User::class, 'assigned_to');
    }

    /** @return HasMany<ReportNote, $this> */
    public function notes(): HasMany
    {
        return $this->hasMany(ReportNote::class)->latest();
    }

    /** @return HasMany<AdminAudit, $this> */
    public function activity(): HasMany
    {
        return $this->hasMany(AdminAudit::class, 'target_id')
            ->where('target_type', self::class)
            ->latest();
    }

    /** @return HasMany<FileReport, $this> */
    public function relatedReports(): HasMany
    {
        return $this->hasMany(self::class, 'transfer_identifier', 'transfer_identifier')
            ->whereKeyNot($this->id)
            ->latest();
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function active(Builder $query): void
    {
        $query->whereIn('status', [ReportStatus::Open, ReportStatus::InReview]);
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function closed(Builder $query): void
    {
        $query->whereIn('status', [ReportStatus::Resolved, ReportStatus::Dismissed]);
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function assignedTo(Builder $query, int $userId): void
    {
        $query->where('assigned_to', $userId);
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function unassigned(Builder $query): void
    {
        $query->whereNull('assigned_to');
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'status' => ReportStatus::class,
            'resolved_at' => 'immutable_datetime',
        ];
    }
}
