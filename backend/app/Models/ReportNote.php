<?php

declare(strict_types=1);

namespace App\Models;

use Database\Factories\ReportNoteFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Concerns\HasUlids;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

#[Fillable(['file_report_id', 'author_id', 'body'])]
class ReportNote extends Model
{
    /** @use HasFactory<ReportNoteFactory> */
    use HasFactory, HasUlids;

    /** @return BelongsTo<FileReport, $this> */
    public function report(): BelongsTo
    {
        return $this->belongsTo(FileReport::class, 'file_report_id');
    }

    /** @return BelongsTo<User, $this> */
    public function author(): BelongsTo
    {
        return $this->belongsTo(User::class, 'author_id');
    }
}
