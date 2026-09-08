<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Models\AdminAudit;
use App\Models\Filestore;
use App\Models\Plan;
use App\Models\User;
use App\Support\FilestoreRegistry;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Arr;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Gate;
use Illuminate\Support\Facades\Validator;
use Illuminate\Validation\ValidationException;
use Throwable;

class ManagePlan
{
    /** @var array<string> */
    private const array EditableAttributes = [
        'maximum_transfer_bytes',
        'maximum_file_count',
        'maximum_note_bytes',
        'default_file_retention_hours',
        'maximum_file_retention_hours',
        'default_note_retention_hours',
        'maximum_note_retention_hours',
        'is_active',
        'placement_mode',
        'filestore_ids',
        'default_filestore_ids',
    ];

    /**
     * @param  array<string, mixed>  $attributes
     *
     * @throws AuthorizationException
     * @throws ValidationException|Throwable
     */
    public function update(User $actor, Plan $plan, array $attributes): Plan
    {
        Gate::forUser($actor)->authorize('update', $plan);

        $unexpectedAttributes = array_diff(array_keys($attributes), self::EditableAttributes);

        if ($unexpectedAttributes !== []) {
            throw ValidationException::withMessages(['attributes' => 'Only plan limits, retention settings, availability, and storage placement may be changed.']);
        }

        $validated = Validator::make($attributes, [
            'maximum_transfer_bytes' => ['required', 'integer', 'min:1', 'max:9223372036854775807'],
            'maximum_file_count' => ['required', 'integer', 'min:1', 'max:32767'],
            'maximum_note_bytes' => ['required', 'integer', 'min:1', 'max:9223372036854775807'],
            'default_file_retention_hours' => ['required', 'integer', 'min:1', 'max:2147483647', 'lte:maximum_file_retention_hours'],
            'maximum_file_retention_hours' => ['required', 'integer', 'min:1', 'max:2147483647'],
            'default_note_retention_hours' => ['required', 'integer', 'min:1', 'max:2147483647', 'lte:maximum_note_retention_hours'],
            'maximum_note_retention_hours' => ['required', 'integer', 'min:1', 'max:2147483647'],
            'is_active' => ['required', 'boolean'],
            'placement_mode' => ['sometimes', 'required', 'in:distribute,replicate'],
            'filestore_ids' => ['sometimes', 'required', 'array', 'min:1'],
            'filestore_ids.*' => ['integer', 'distinct', 'exists:filestores,id'],
            'default_filestore_ids' => ['sometimes', 'required', 'array', 'min:1'],
            'default_filestore_ids.*' => ['integer', 'distinct', 'exists:filestores,id'],
        ])->validate();

        $validated['is_active'] = filter_var($validated['is_active'], FILTER_VALIDATE_BOOLEAN);

        return DB::transaction(function () use ($actor, $plan, $validated): Plan {
            $actor = User::query()->lockForUpdate()->findOrFail($actor->id);
            $plan = Plan::query()->lockForUpdate()->findOrFail($plan->id);
            Gate::forUser($actor)->authorize('update', $plan);

            $managesFilestores = array_key_exists('filestore_ids', $validated) || array_key_exists('default_filestore_ids', $validated);
            $existingAssignments = $managesFilestores
                ? $plan->filestores()->get()->mapWithKeys(fn (Filestore $filestore): array => [(int) $filestore->id => (bool) $filestore->getAttribute('pivot')?->getAttribute('is_default')])
                : collect();
            $existingFilestoreIds = array_map('intval', $existingAssignments->keys()->all());
            $filestoreIds = $managesFilestores
                ? array_map('intval', $validated['filestore_ids'] ?? $existingFilestoreIds)
                : [];
            if ($managesFilestores && $filestoreIds !== []) {
                $lockedFilestores = Filestore::query()
                    ->whereKey(array_values(array_unique([...$existingFilestoreIds, ...$filestoreIds])))
                    ->orderBy('id')
                    ->lockForUpdate()
                    ->get();
                $filestores = $lockedFilestores->filter(fn (Filestore $filestore): bool => in_array((int) $filestore->id, $filestoreIds, true));
                if ($filestores->count() !== count($filestoreIds)) {
                    throw ValidationException::withMessages(['filestore_ids' => 'Select valid filestores.']);
                }
            } else {
                $filestores = collect();
            }

            if ($plan->slug === config('filebeam.transfers.default_plan') && ! $validated['is_active']) {
                throw ValidationException::withMessages(['is_active' => 'The configured default plan cannot be disabled.']);
            }

            $changes = [];

            foreach (array_diff(self::EditableAttributes, ['filestore_ids', 'default_filestore_ids']) as $attribute) {
                if (! array_key_exists($attribute, $validated)) {
                    continue;
                }
                if ($plan->getAttribute($attribute) != $validated[$attribute]) {
                    $changes[$attribute] = ['from' => $plan->getAttribute($attribute), 'to' => $validated[$attribute]];
                }
            }

            $plan->forceFill(Arr::only($validated, array_diff(self::EditableAttributes, ['filestore_ids', 'default_filestore_ids'])))->save();

            if ($managesFilestores) {
                $defaultFilestoreIds = array_map('intval', $validated['default_filestore_ids'] ?? $existingAssignments->filter()->keys()->all());
                if ($defaultFilestoreIds === []) {
                    throw ValidationException::withMessages(['default_filestore_ids' => 'Select at least one default store.']);
                }
                if (array_diff($defaultFilestoreIds, $filestoreIds) !== []) {
                    throw ValidationException::withMessages(['default_filestore_ids' => 'Every default store must be in the allowed store subset.']);
                }
                $registry = app(FilestoreRegistry::class);
                if ($registry->environmentManaged() && $filestores->contains(fn (Filestore $filestore): bool => ! $registry->placementEnabled($filestore))) {
                    throw ValidationException::withMessages(['filestore_ids' => 'Environment-managed plans may only use stores from the configured environment pool.']);
                }
                if ($filestores->filter(fn (Filestore $filestore): bool => in_array((int) $filestore->id, $defaultFilestoreIds, true))->contains(fn (Filestore $filestore): bool => ! $registry->placementEnabled($filestore))) {
                    throw ValidationException::withMessages(['default_filestore_ids' => 'Every default store must currently be available for placement.']);
                }
                $plan->filestores()->sync(collect($filestoreIds)->mapWithKeys(fn (int $id): array => [$id => ['is_default' => in_array($id, $defaultFilestoreIds, true)]])->all());
                $changes['filestores'] = [
                    'from' => ['ids' => $existingFilestoreIds, 'default_ids' => array_map('intval', $existingAssignments->filter()->keys()->all())],
                    'to' => ['ids' => $filestoreIds, 'default_ids' => $defaultFilestoreIds],
                ];
            }

            AdminAudit::query()->create([
                'actor_id' => $actor->getKey(),
                'action' => 'plan.updated',
                'target_type' => $plan::class,
                'target_id' => $plan->getKey(),
                'changes' => $changes,
            ]);

            return $plan;
        });
    }
}
