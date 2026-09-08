<?php

declare(strict_types=1);

namespace App\Enums;

use Filament\Support\Contracts\HasLabel;

enum TransferKind: string implements HasLabel
{
    case Files = 'files';
    case Note = 'note';

    public function getLabel(): string
    {
        return match ($this) {
            self::Files => 'Files',
            self::Note => 'Note',
        };
    }
}
