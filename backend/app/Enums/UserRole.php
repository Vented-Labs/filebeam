<?php

declare(strict_types=1);

namespace App\Enums;

use Filament\Support\Contracts\HasColor;
use Filament\Support\Contracts\HasLabel;

enum UserRole: string implements HasColor, HasLabel
{
    case User = 'user';
    case Moderator = 'moderator';
    case Admin = 'admin';

    public function getLabel(): string
    {
        return match ($this) {
            self::User => 'User',
            self::Moderator => 'Moderator',
            self::Admin => 'Administrator',
        };
    }

    public function getColor(): string
    {
        return match ($this) {
            self::User => 'gray',
            self::Moderator => 'info',
            self::Admin => 'primary',
        };
    }
}
