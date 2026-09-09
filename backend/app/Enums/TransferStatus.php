<?php

declare(strict_types=1);

namespace App\Enums;

use Filament\Support\Contracts\HasColor;
use Filament\Support\Contracts\HasLabel;

enum TransferStatus: string implements HasColor, HasLabel
{
    case Pending = 'pending';
    case Available = 'available';
    case Live = 'live';
    case Ended = 'ended';
    case Deleting = 'deleting';

    public function getLabel(): string
    {
        return match ($this) {
            self::Pending => 'Uploading',
            self::Available => 'Available',
            self::Live => 'Live',
            self::Ended => 'Ended',
            self::Deleting => 'Awaiting cleanup',
        };
    }

    public function getColor(): string
    {
        return match ($this) {
            self::Pending => 'info',
            self::Available => 'success',
            self::Live => 'success',
            self::Ended => 'gray',
            self::Deleting => 'warning',
        };
    }
}
