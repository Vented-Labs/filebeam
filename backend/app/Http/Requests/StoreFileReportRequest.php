<?php

declare(strict_types=1);

namespace App\Http\Requests;

use Illuminate\Contracts\Validation\ValidationRule;
use Illuminate\Contracts\Validation\Validator;
use Illuminate\Foundation\Http\FormRequest;

class StoreFileReportRequest extends FormRequest
{
    public function authorize(): bool
    {
        return true;
    }

    /**
     * @return array<string, list<ValidationRule|string>>
     */
    public function rules(): array
    {
        return [
            'transfer_id' => ['required', 'ulid'],
            'category' => ['required', 'string', 'in:spam,malware,illegal_content,privacy,copyright,other'],
            'description' => ['required', 'string', 'min:10', 'max:2000'],
            'reporter_email' => ['nullable', 'email', 'max:254'],
            'website' => ['nullable', 'string', 'max:200'],
        ];
    }

    protected function failedValidation(Validator $validator): void
    {
        app('request')->replace($this->only([
            'transfer_id',
            'category',
            'description',
            'reporter_email',
        ]));

        parent::failedValidation($validator);
    }
}
