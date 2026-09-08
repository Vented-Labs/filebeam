<?php

declare(strict_types=1);

namespace App\Enums;

use Filament\Support\Contracts\HasColor;
use Filament\Support\Contracts\HasLabel;

enum ReportStatus: string implements HasColor, HasLabel
{
    case Open = 'open';
    case InReview = 'in_review';
    case Resolved = 'resolved';
    case Dismissed = 'dismissed';

    public function getLabel(): string
    {
        return match ($this) {
            self::Open => 'Open',
            self::InReview => 'In review',
            self::Resolved => 'Resolved',
            self::Dismissed => 'Dismissed',
        };
    }

    public function getColor(): string
    {
        return match ($this) {
            self::Open => 'warning',
            self::InReview => 'info',
            self::Resolved => 'success',
            self::Dismissed => 'gray',
        };
    }
}
