<?php

declare(strict_types=1);

namespace App\Support;

class Capability
{
    public static function matches(?string $expectedHash, ?string $token): bool
    {
        return $expectedHash !== null
            && is_string($token)
            && hash_equals($expectedHash, hash('sha256', $token));
    }
}
