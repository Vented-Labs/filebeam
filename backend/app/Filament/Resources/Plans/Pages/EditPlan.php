<?php

declare(strict_types=1);

namespace App\Filament\Resources\Plans\Pages;

use App\Actions\Admin\ManagePlan;
use App\Enums\TransferDriver;
use App\Filament\Resources\Plans\PlanResource;
use App\Models\Plan;
use App\Models\User;
use App\Support\TransportPolicy;
use Filament\Facades\Filament;
use Filament\Resources\Pages\EditRecord;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Validation\ValidationException;

class EditPlan extends EditRecord
{
    protected static string $resource = PlanResource::class;

    /** @return array<never> */
    protected function getAllRelationManagers(): array
    {
        return [];
    }

    /**
     * @param  array<string, mixed>  $data
     * @return array<string, mixed>
     */
    protected function mutateFormDataBeforeFill(array $data): array
    {
        /** @var Plan $plan */
        $plan = $this->getRecord();
        $transfer = PlanResource::humanBytes($plan->maximum_transfer_bytes);
        $note = PlanResource::humanBytes($plan->maximum_note_bytes);
        $webrtcTransfer = $plan->webrtc_maximum_transfer_bytes === null ? null : PlanResource::humanBytes($plan->webrtc_maximum_transfer_bytes);
        $webrtcNote = $plan->webrtc_maximum_note_bytes === null ? null : PlanResource::humanBytes($plan->webrtc_maximum_note_bytes);
        $defaultFile = PlanResource::humanHours($plan->default_file_retention_hours);
        $maximumFile = PlanResource::humanHours($plan->maximum_file_retention_hours);
        $defaultNote = PlanResource::humanHours($plan->default_note_retention_hours);
        $maximumNote = PlanResource::humanHours($plan->maximum_note_retention_hours);

        return [
            'maximum_transfer_quantity' => $transfer['quantity'], 'maximum_transfer_unit' => $transfer['unit'],
            'maximum_file_count' => $plan->maximum_file_count,
            'maximum_note_quantity' => $note['quantity'], 'maximum_note_unit' => $note['unit'],
            'webrtc_maximum_transfer_unlimited' => $webrtcTransfer === null,
            'webrtc_maximum_transfer_quantity' => $webrtcTransfer['quantity'] ?? null, 'webrtc_maximum_transfer_unit' => $webrtcTransfer['unit'] ?? 'B',
            'webrtc_maximum_file_count_unlimited' => $plan->webrtc_maximum_file_count === null,
            'webrtc_maximum_file_count' => $plan->webrtc_maximum_file_count,
            'webrtc_maximum_note_unlimited' => $webrtcNote === null,
            'webrtc_maximum_note_quantity' => $webrtcNote['quantity'] ?? null, 'webrtc_maximum_note_unit' => $webrtcNote['unit'] ?? 'B',
            'default_file_retention_quantity' => $defaultFile['quantity'], 'default_file_retention_unit' => $defaultFile['unit'],
            'maximum_file_retention_quantity' => $maximumFile['quantity'], 'maximum_file_retention_unit' => $maximumFile['unit'],
            'default_note_retention_quantity' => $defaultNote['quantity'], 'default_note_retention_unit' => $defaultNote['unit'],
            'maximum_note_retention_quantity' => $maximumNote['quantity'], 'maximum_note_retention_unit' => $maximumNote['unit'],
            'is_active' => $plan->is_active,
            'placement_mode' => $plan->placement_mode,
            'filestore_ids' => $plan->filestores()->pluck('filestores.id')->all(),
            'default_filestore_ids' => $plan->filestores()->wherePivot('is_default', true)->pluck('filestores.id')->all(),
        ];
    }

    /**
     * @param  array<string, mixed>  $data
     * @return array<string, mixed>
     */
    protected function mutateFormDataBeforeSave(array $data): array
    {
        $attributes = [
            'maximum_transfer_bytes' => $this->bytes($data, 'maximum_transfer'),
            'maximum_file_count' => $data['maximum_file_count'],
            'maximum_note_bytes' => $this->bytes($data, 'maximum_note'),
            'webrtc_maximum_transfer_bytes' => (bool) $data['webrtc_maximum_transfer_unlimited'] ? null : $this->bytes($data, 'webrtc_maximum_transfer'),
            'webrtc_maximum_file_count' => (bool) $data['webrtc_maximum_file_count_unlimited'] ? null : $data['webrtc_maximum_file_count'],
            'webrtc_maximum_note_bytes' => (bool) $data['webrtc_maximum_note_unlimited'] ? null : $this->bytes($data, 'webrtc_maximum_note'),
            'default_file_retention_hours' => $this->hours($data, 'default_file_retention'),
            'maximum_file_retention_hours' => $this->hours($data, 'maximum_file_retention'),
            'default_note_retention_hours' => $this->hours($data, 'default_note_retention'),
            'maximum_note_retention_hours' => $this->hours($data, 'maximum_note_retention'),
            'is_active' => $data['is_active'],
            'placement_mode' => $data['placement_mode'],
            'filestore_ids' => $data['filestore_ids'],
            'default_filestore_ids' => $data['default_filestore_ids'],
        ];

        if ($attributes['default_file_retention_hours'] > $attributes['maximum_file_retention_hours']) {
            throw ValidationException::withMessages(['data.default_file_retention_quantity' => 'Default file retention cannot exceed the maximum file retention.']);
        }

        if ($attributes['default_note_retention_hours'] > $attributes['maximum_note_retention_hours']) {
            throw ValidationException::withMessages(['data.default_note_retention_quantity' => 'Default note retention cannot exceed the maximum note retention.']);
        }

        if ($attributes['filestore_ids'] === [] && $attributes['default_filestore_ids'] === [] && app(TransportPolicy::class)->allows(TransferDriver::Http)) {
            unset($attributes['filestore_ids'], $attributes['default_filestore_ids']);
        }

        return $attributes;
    }

    protected function handleRecordUpdate(Model $record, array $data): Model
    {
        if (! $record instanceof Plan) {
            throw new \LogicException('Plan editing requires a plan record.');
        }

        $actor = Filament::auth()->user();
        abort_unless($actor instanceof User, 403);

        return app(ManagePlan::class)->update($actor, $record, $data);
    }

    /** @param array<string, mixed> $data */
    private function bytes(array $data, string $prefix): int
    {
        $multiplier = PlanResource::byteMultiplier($data["{$prefix}_unit"]);
        $quantity = $this->quantity($data["{$prefix}_quantity"], intdiv(PHP_INT_MAX, $multiplier), "{$prefix}_quantity");

        return $quantity * $multiplier;
    }

    /** @param array<string, mixed> $data */
    private function hours(array $data, string $prefix): int
    {
        $multiplier = PlanResource::durationMultiplier($data["{$prefix}_unit"]);
        $quantity = $this->quantity($data["{$prefix}_quantity"], intdiv(2147483647, $multiplier), "{$prefix}_quantity");

        return $quantity * $multiplier;
    }

    private function quantity(mixed $quantity, int $maximum, string $field): int
    {
        $quantity = is_int($quantity) ? (string) $quantity : $quantity;

        if (! is_string($quantity) || ! ctype_digit($quantity) || $quantity === '0') {
            throw ValidationException::withMessages(["data.{$field}" => 'Enter a whole number greater than zero.']);
        }

        $maximumString = (string) $maximum;

        if (strlen($quantity) > strlen($maximumString) || (strlen($quantity) === strlen($maximumString) && strcmp($quantity, $maximumString) > 0)) {
            throw ValidationException::withMessages(["data.{$field}" => 'This value is too large for the selected unit.']);
        }

        return (int) $quantity;
    }

    protected function getRedirectUrl(): ?string
    {
        return PlanResource::getUrl('view', ['record' => $this->getRecord()]);
    }

    protected function getSavedNotificationTitle(): ?string
    {
        return 'Plan updated';
    }
}
