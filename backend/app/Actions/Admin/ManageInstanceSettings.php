<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Models\AdminAudit;
use App\Models\InstanceSetting;
use App\Models\User;
use App\Support\InstanceSettings;
use App\Support\InstanceSettingValue;
use App\Support\TransportPolicy;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\DB;
use Illuminate\Validation\ValidationException;
use InvalidArgumentException;
use Throwable;

class ManageInstanceSettings
{
    /**
     * @param  array<string, mixed>  $values  keyed by registered setting; null or '' removes the database override
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

            try {
                $normalizedValues[$key] = InstanceSettingValue::normalize($definitions[$key]['type'], $value);
            } catch (InvalidArgumentException $exception) {
                throw ValidationException::withMessages([$key => $exception->getMessage()]);
            }
        }

        DB::transaction(function () use ($actor, $settings, $normalizedValues): void {
            $lockedActor = User::query()->whereKey($actor->getKey())->lockForUpdate()->firstOrFail();

            if (! $lockedActor->isAdmin()) {
                throw new AuthorizationException;
            }

            $this->assertTransportPolicy($settings, $normalizedValues);

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

    /**
     * The transport keys are validated together so the effective default driver stays enabled.
     *
     * @param  array<string, mixed>  $submitted
     *
     * @throws ValidationException
     */
    private function assertTransportPolicy(InstanceSettings $settings, array $submitted): void
    {
        $keys = ['enabled_drivers', 'default_driver'];

        if (array_intersect(array_keys($submitted), $keys) === []) {
            return;
        }

        $effective = $settings->values($keys);

        foreach ($keys as $key) {
            if (array_key_exists($key, $submitted) && $submitted[$key] !== null) {
                $effective[$key] = $submitted[$key];
            }
        }

        try {
            app(TransportPolicy::class)->validate($effective['enabled_drivers'], $effective['default_driver']);
        } catch (InvalidArgumentException $exception) {
            throw ValidationException::withMessages(['transport_policy' => $exception->getMessage()]);
        }
    }
}
