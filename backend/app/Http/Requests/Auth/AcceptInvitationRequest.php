<?php

declare(strict_types=1);

namespace App\Http\Requests\Auth;

use App\Rules\ReservedUsername;
use Illuminate\Foundation\Http\FormRequest;
use Illuminate\Validation\Rule;
use Illuminate\Validation\Rules\Password;

class AcceptInvitationRequest extends FormRequest
{
    public function authorize(): bool
    {
        return true;
    }

    /** @return array<string, list<mixed>> */
    public function rules(): array
    {
        return [
            'username' => ['required', 'string', 'regex:/\A[a-z0-9_]{3,24}\z/', new ReservedUsername, Rule::unique('users', 'normalized_username')],
            'name' => ['nullable', 'string', 'max:255'],
            'email' => ['required', 'string', 'email:rfc', 'max:255', Rule::unique('users', 'email')],
            'password' => ['required', 'confirmed', Password::defaults()],
        ];
    }

    protected function prepareForValidation(): void
    {
        $username = $this->input('username');
        $email = $this->input('email');
        $name = $this->input('name');

        $this->merge([
            'username' => is_string($username) ? strtolower(trim($username)) : $username,
            'email' => is_string($email) ? strtolower(trim($email)) : $email,
            'name' => is_string($name) ? trim($name) ?: null : $name,
        ]);
    }
}
