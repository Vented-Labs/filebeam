<?php

declare(strict_types=1);

namespace App\Http\Requests\Auth;

use Illuminate\Foundation\Http\FormRequest;

class ForgotPasswordRequest extends FormRequest
{
    protected $redirectRoute = 'password.request';

    public function authorize(): bool
    {
        return true;
    }

    /** @return array<string, list<string>> */
    public function rules(): array
    {
        return ['email' => ['required', 'string', 'max:255']];
    }

    protected function prepareForValidation(): void
    {
        $email = $this->input('email');

        $this->merge(['email' => is_string($email) ? strtolower(trim($email)) : $email]);
    }
}
