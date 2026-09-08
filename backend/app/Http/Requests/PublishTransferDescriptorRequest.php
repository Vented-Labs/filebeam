<?php

declare(strict_types=1);

namespace App\Http\Requests;

use Illuminate\Contracts\Validation\ValidationRule;
use Illuminate\Foundation\Http\FormRequest;

class PublishTransferDescriptorRequest extends FormRequest
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
        ];
    }
}
