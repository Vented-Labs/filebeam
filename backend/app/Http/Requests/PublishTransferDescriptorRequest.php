<?php

declare(strict_types=1);

namespace App\Http\Requests;

use Illuminate\Contracts\Validation\ValidationRule;

class PublishTransferDescriptorRequest extends CompleteTransferRequest
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
            'encrypted_descriptor' => ['required', 'string', 'max:524288'],
            'encrypted_key' => ['nullable', 'string', 'regex:/\A[A-Za-z0-9_-]{107}\z/'],
        ];
    }
}
