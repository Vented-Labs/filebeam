<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Models\AdminAudit;
use App\Models\InstanceTransportPolicy;
use App\Models\User;
use App\Support\TransportPolicy;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\DB;
use Illuminate\Validation\ValidationException;
use Throwable;

class ManageTransportPolicy
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

        if (array_diff(array_keys($values), ['enabled_drivers', 'default_driver']) !== []) {
            throw ValidationException::withMessages(['transport_policy' => 'Only transfer driver settings may be changed.']);
        }

        $environment = config('filebeam.transport_policy.environment');
        foreach (array_keys($values) as $key) {
            if ($environment[$key] !== null) {
                throw ValidationException::withMessages([$key => 'This setting is controlled by the environment and cannot be overridden in Admin.']);
            }
        }

        DB::transaction(function () use ($actor, $values, $environment): void {
            $lockedActor = User::query()->whereKey($actor->getKey())->lockForUpdate()->firstOrFail();
            if (! $lockedActor->isAdmin()) {
                throw new AuthorizationException;
            }

            $policy = InstanceTransportPolicy::query()->lockForUpdate()->find(1)
                ?? tap(new InstanceTransportPolicy, function (InstanceTransportPolicy $policy): void {
                    $policy->forceFill([
                        'id' => 1,
                        'enabled_drivers' => config('filebeam.transport_policy.defaults.enabled_drivers'),
                        'default_driver' => config('filebeam.transport_policy.defaults.default_driver'),
                    ])->save();
                });
            $candidate = [
                'enabled_drivers' => $values['enabled_drivers'] ?? $policy->enabled_drivers,
                'default_driver' => $values['default_driver'] ?? $policy->default_driver,
            ];

            try {
                app(TransportPolicy::class)->validate(
                    $environment['enabled_drivers'] ?? $candidate['enabled_drivers'],
                    $environment['default_driver'] ?? $candidate['default_driver'],
                );
            } catch (\InvalidArgumentException $exception) {
                throw ValidationException::withMessages(['transport_policy' => $exception->getMessage()]);
            }

            $changes = [];
            foreach ($values as $key => $value) {
                if ($policy->getAttribute($key) !== $value) {
                    $changes[$key] = ['from' => $policy->getAttribute($key), 'to' => $value];
                }
            }
            if ($changes === []) {
                return;
            }

            $policy->forceFill($values)->save();
            AdminAudit::query()->create([
                'actor_id' => $lockedActor->getKey(),
                'action' => 'instance_transport_policy.updated',
                'target_type' => InstanceTransportPolicy::class,
                'target_id' => $policy->getKey(),
                'changes' => $changes,
            ]);
        });
    }
}
