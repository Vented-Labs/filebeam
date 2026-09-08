<?php

declare(strict_types=1);

namespace App\Rules;

use Closure;
use Illuminate\Contracts\Validation\ValidationRule;

class ReservedUsername implements ValidationRule
{
    /** @var list<string> */
    private const array Reserved = [
        'account', 'api', 'boost', 'email', 'forgot-password', 'login', 'logout',
        'register', 'reset-password', 'storage', 'u', 'up', 'verify-email',
    ];

    /**
     * @param  Closure(string): mixed  $fail
     */
    public function validate(string $attribute, mixed $value, Closure $fail): void
    {
        if (is_string($value) && in_array(strtolower($value), self::Reserved, true)) {
            $fail('This username is reserved.');
        }
    }
}
