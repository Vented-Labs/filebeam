<?php

declare(strict_types=1);

namespace App\Enums;

enum Feature: string
{
    case CustomThemes = 'custom_themes';
    case RedisCache = 'redis_cache';

    /** @return list<string> */
    public function extensions(): array
    {
        return match ($this) {
            self::CustomThemes => ['gd'],
            self::RedisCache => ['redis'],
        };
    }
}
