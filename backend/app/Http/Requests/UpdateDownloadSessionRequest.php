<?php

declare(strict_types=1);

namespace App\Http\Requests;

use Illuminate\Contracts\Validation\ValidationRule;
use Illuminate\Foundation\Http\FormRequest;

class UpdateDownloadSessionRequest extends FormRequest
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
            'sequence' => ['required', 'integer', 'min:1'],
            'progress' => ['required', 'numeric', 'between:0,100'],
            'status' => ['required', 'string', 'in:downloading,waiting,verifying,completed,cancelled,error'],
        ];
    }
}
