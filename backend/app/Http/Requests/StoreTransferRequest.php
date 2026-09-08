<?php

declare(strict_types=1);

namespace App\Http\Requests;

use App\Enums\TransferKind;
use App\Models\Plan;
use App\Support\EffectivePlan;
use App\Support\InstanceSettings;
use Closure;
use Illuminate\Contracts\Container\BindingResolutionException;
use Illuminate\Contracts\Container\CircularDependencyException;
use Illuminate\Contracts\Validation\ValidationRule;
use Illuminate\Foundation\Http\FormRequest;
use Illuminate\Validation\Validator;

class StoreTransferRequest extends FormRequest
{
    /**
     * Determine if the user is authorized to make this request.
     */
    public function authorize(): bool
    {
        return app(InstanceSettings::class)->boolean('anonymous_uploads') || $this->user() !== null;
    }

    /**
     * Get the validation rules that apply to the request.
     *
     * @return array<string, list<ValidationRule|string>>
     */
    public function rules(): array
    {
        $plan = $this->plan();
        $maximumRetentionHours = $this->input('kind') === TransferKind::Note->value
            ? $plan->maximum_note_retention_hours
            : $plan->maximum_file_retention_hours;

        return [
            'kind' => ['required', 'string', 'in:files,note'],
            'protocol_version' => ['required', 'integer', 'in:1'],
            'chunk_bytes' => ['required', 'integer', 'in:'.config('filebeam.transfers.chunk_bytes')],
            'items' => ['required', 'array', 'min:1', 'max:100'],
            'items.*.ciphertext_bytes' => ['required', 'integer', 'min:16'],
            'items.*.chunk_count' => ['required', 'integer', 'min:1', 'max:65535'],
            'retention_hours' => ['nullable', 'integer', 'min:1', "max:{$maximumRetentionHours}"],
            'burn_on_read' => ['nullable', 'boolean'],
            'recipient_username' => ['nullable', 'required_with:account_key_bundle_id', 'string', 'regex:/\A[a-z0-9_]{3,24}\z/'],
            'account_key_bundle_id' => ['nullable', 'required_with:recipient_username', 'integer'],
            'filestore_ids' => ['prohibited'],
            'placement_mode' => ['prohibited'],
        ];
    }

    /**
     * @throws CircularDependencyException
     * @throws BindingResolutionException
     */
    private function plan(): Plan
    {
        return app(EffectivePlan::class)->resolve($this->user());
    }

    /** @return array<int, Closure(Validator): void> */
    public function after(): array
    {
        return [function (Validator $validator): void {
            if ($validator->errors()->isNotEmpty()) {
                return;
            }

            $maximumChunkBytes = (int) config('filebeam.transfers.chunk_bytes') + 16;

            foreach ($this->input('items', []) as $position => $item) {
                $minimumBytes = (($item['chunk_count'] - 1) * $maximumChunkBytes) + ($item['chunk_count'] === 1 ? 16 : 17);
                $maximumBytes = $item['chunk_count'] * $maximumChunkBytes;

                if ($item['ciphertext_bytes'] < $minimumBytes || $item['ciphertext_bytes'] > $maximumBytes) {
                    $validator->errors()->add(
                        "items.{$position}.ciphertext_bytes",
                        'The declared ciphertext size cannot be represented by the declared chunks.',
                    );
                }
            }

            if ($this->boolean('burn_on_read') && $this->input('kind') !== TransferKind::Note->value) {
                $validator->errors()->add('burn_on_read', 'Burn on read is only available for note transfers.');
            }
        }];
    }
}
