<?php

declare(strict_types=1);

namespace App\Support;

class AuthIdentifier
{
    public static function normalize(string $identifier): string
    {
        return strtolower(trim($identifier));
    }

    /** @return array<string, string> */
    public static function credentials(string $identifier): array
    {
        $identifier = self::normalize($identifier);

        return str_contains($identifier, '@')
            ? ['email' => $identifier]
            : ['normalized_username' => $identifier];
    }
}
