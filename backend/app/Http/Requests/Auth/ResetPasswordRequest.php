<?php

declare(strict_types=1);

namespace App\Http\Requests\Auth;

use Illuminate\Foundation\Http\FormRequest;
use Illuminate\Validation\Rules\Password;

class ResetPasswordRequest extends FormRequest
{
    protected function getRedirectUrl(): string
    {
        $token = $this->input('token');

        return is_string($token) && $token !== ''
            ? route('password.reset', ['token' => $token])
            : route('password.request');
    }

    public function authorize(): bool
    {
        return true;
    }

    /** @return array<string, list<mixed>> */
    public function rules(): array
    {
        return [
            'token' => ['required', 'string'],
            'email' => ['required', 'string', 'max:255'],
            'password' => ['required', 'confirmed', Password::defaults()],
        ];
    }

    protected function prepareForValidation(): void
    {
        $email = $this->input('email');

        $this->merge(['email' => is_string($email) ? strtolower(trim($email)) : $email]);
    }
}
