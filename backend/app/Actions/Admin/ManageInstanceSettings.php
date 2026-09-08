<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Models\AdminAudit;
use App\Models\InstanceSetting;
use App\Models\User;
use App\Support\InstanceSettings;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\DB;
use Illuminate\Validation\ValidationException;
use Throwable;

class ManageInstanceSettings
{
    /**
     * @param  array<string, mixed>  $values
     *
     * @throws AuthorizationException
     * @throws ValidationException
     * @throws Throwable
     */
    public function update(User $actor, array $values): void
    {
        if (! $actor->isAdmin()) {
            throw new AuthorizationException;
        }

        $settings = app(InstanceSettings::class);
        $definitions = $settings->definitions();
        $unexpectedKeys = array_diff(array_keys($values), array_keys($definitions));

        if ($unexpectedKeys !== []) {
            throw ValidationException::withMessages(['settings' => 'Only registered instance settings may be changed.']);
        }

        $normalizedValues = [];

        foreach ($values as $key => $value) {
            if ($settings->environmentValue($key) !== null) {
                throw ValidationException::withMessages([$key => 'This setting is controlled by the environment and cannot be overridden in Admin.']);
            }

            $normalizedValues[$key] = $this->normalizeBoolean($key, $value);
        }

        DB::transaction(function () use ($actor, $normalizedValues): void {
            $lockedActor = User::query()->whereKey($actor->getKey())->lockForUpdate()->firstOrFail();

            if (! $lockedActor->isAdmin()) {
                throw new AuthorizationException;
            }

            foreach ($normalizedValues as $key => $value) {
                $setting = InstanceSetting::query()->lockForUpdate()->find($key);
                $from = $setting?->value;

                if ($from === $value) {
                    continue;
                }

                if ($value === null) {
                    $setting?->delete();
                } else {
                    InstanceSetting::query()->updateOrCreate(['key' => $key], ['value' => $value]);
                }

                AdminAudit::query()->create([
                    'actor_id' => $lockedActor->getKey(),
                    'action' => 'instance_setting.updated',
                    'target_type' => InstanceSetting::class,
                    'target_id' => $key,
                    'changes' => ['value' => ['from' => $from, 'to' => $value]],
                ]);
            }
        });
    }

    /** @throws ValidationException */
    private function normalizeBoolean(string $key, mixed $value): ?bool
    {
        if ($value === null || $value === '') {
            return null;
        }

        if (is_bool($value)) {
            return $value;
        }

        if ($value === '1' || $value === 1) {
            return true;
        }

        if ($value === '0' || $value === 0) {
            return false;
        }

        throw ValidationException::withMessages([$key => 'Select Enabled, Disabled, or Inherit.']);
    }
}
