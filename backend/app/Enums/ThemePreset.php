<?php

declare(strict_types=1);

namespace App\Enums;

enum ThemePreset: string
{
    case Instance = 'instance';
    case Purple = 'purple';
    case Blue = 'blue';
    case Teal = 'teal';
    case Green = 'green';
    case Amber = 'amber';
    case Orange = 'orange';
    case Rose = 'rose';

    public function label(): string
    {
        return $this === self::Instance ? 'Instance default' : ucfirst($this->value);
    }

    public function primary(string $instance): string
    {
        return match ($this) {
            self::Instance => $instance,
            self::Purple => '#8b35ff',
            self::Blue => '#3b82f6',
            self::Teal => '#14b8a6',
            self::Green => '#22c55e',
            self::Amber => '#f59e0b',
            self::Orange => '#f97316',
            self::Rose => '#f43f5e',
        };
    }
}
