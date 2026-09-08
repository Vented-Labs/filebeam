<?php

declare(strict_types=1);

namespace App\Http\Requests;

use Closure;
use Illuminate\Contracts\Validation\ValidationRule;
use Illuminate\Foundation\Http\FormRequest;
use Illuminate\Validation\Validator;

class CompleteTransferRequest extends FormRequest
{
    /**
     * Determine if the user is authorized to make this request.
     */
    public function authorize(): bool
    {
        return true;
    }

    /**
     * Get the validation rules that apply to the request.
     *
     * @return array<string, list<ValidationRule|string>>
     */
    public function rules(): array
    {
        return [
            'encrypted_manifest' => ['required', 'string', 'max:524288'],
            'encrypted_key' => ['nullable', 'string', 'regex:/\A[A-Za-z0-9_-]{107}\z/'],
        ];
    }

    /** @return array<int, Closure(Validator): void> */
    public function after(): array
    {
        return [function (Validator $validator): void {
            $encryptedKey = $this->input('encrypted_key');

            if (! is_string($encryptedKey)) {
                return;
            }

            $decoded = base64_decode(strtr($encryptedKey, '-_', '+/'), true);
            $canonical = is_string($decoded)
                ? $decoded
                    |> base64_encode(...)
                    |> (fn ($x) => strtr($x, '+/', '-_'))
                    |> (fn ($x) => rtrim($x, '='))
                : null;

            if (! is_string($decoded) || strlen($decoded) !== 80 || ! hash_equals($canonical ?? '', $encryptedKey)) {
                $validator->errors()->add('encrypted_key', 'The recipient key must be an 80-byte base64url value.');
            }
        }];
    }
}
