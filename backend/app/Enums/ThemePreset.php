<?php

declare(strict_types=1);

namespace App\Enums;

use App\Support\Theming\ThemeDefinition;

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
        return $this === self::Instance ? $instance : ThemeDefinition::SEEDS[$this->value];
    }
}
