<?php

declare(strict_types=1);

namespace App\Support;

use App\Enums\TransferDriver;
use App\Models\InstanceTransportPolicy;
use App\Models\Plan;
use App\Models\User;
use InvalidArgumentException;

class TransportPolicy
{
    /** @return array{enabled_drivers: list<string>, default_driver: string} */
    public function resolve(): array
    {
        $stored = InstanceTransportPolicy::query()->find(1);
        $enabledDrivers = config('filebeam.transport_policy.environment.enabled_drivers')
            ?? ($stored === null ? null : $stored->enabled_drivers)
            ?? config('filebeam.transport_policy.defaults.enabled_drivers');
        $defaultDriver = config('filebeam.transport_policy.environment.default_driver')
            ?? ($stored === null ? null : $stored->default_driver)
            ?? config('filebeam.transport_policy.defaults.default_driver');

        return $this->validate($enabledDrivers, $defaultDriver);
    }

    public function allows(TransferDriver $driver): bool
    {
        return in_array($driver->value, $this->resolve()['enabled_drivers'], true);
    }

    /** @return array{maximum_transfer_bytes: int|null, maximum_file_count: int|null, maximum_note_bytes: int|null} */
    public function limits(Plan $plan, TransferDriver $driver): array
    {
        if ($driver === TransferDriver::Http) {
            return [
                'maximum_transfer_bytes' => $plan->maximum_transfer_bytes,
                'maximum_file_count' => $plan->maximum_file_count,
                'maximum_note_bytes' => $plan->maximum_note_bytes,
            ];
        }

        return [
            'maximum_transfer_bytes' => $plan->webrtc_maximum_transfer_bytes,
            'maximum_file_count' => $plan->webrtc_maximum_file_count,
            'maximum_note_bytes' => $plan->webrtc_maximum_note_bytes,
        ];
    }

    /** @return array{enabled_drivers: list<string>, default_driver: string, limits: array{http: array{maximum_transfer_bytes: int|null, maximum_file_count: int|null, maximum_note_bytes: int|null}, webrtc: array{maximum_transfer_bytes: int|null, maximum_file_count: int|null, maximum_note_bytes: int|null}}} */
    public function configuration(?User $user): array
    {
        $policy = $this->resolve();
        $plans = app(EffectivePlan::class);
        $plan = $user === null ? $plans->default() : ($user->plan ?? $plans->default());

        if ($plan === null) {
            $http = [
                'maximum_transfer_bytes' => config('filebeam.default_plan.maximum_transfer_bytes'),
                'maximum_file_count' => config('filebeam.default_plan.maximum_file_count'),
                'maximum_note_bytes' => config('filebeam.default_plan.maximum_note_bytes'),
            ];
            $webrtc = $http;
        } else {
            $http = $this->limits($plan, TransferDriver::Http);
            $webrtc = $this->limits($plan, TransferDriver::WebRtc);
        }

        return [...$policy, 'limits' => ['http' => $http, 'webrtc' => $webrtc]];
    }

    /**
     * @return array{enabled_drivers: list<string>, default_driver: string}
     */
    public function validate(mixed $enabledDrivers, mixed $defaultDriver): array
    {
        if (! is_array($enabledDrivers) || ! is_string($defaultDriver)) {
            throw new InvalidArgumentException('Transfer driver policy must contain enabled drivers and a default driver.');
        }

        $known = array_map(static fn (TransferDriver $driver): string => $driver->value, TransferDriver::cases());
        $enabledDrivers = array_values(array_unique($enabledDrivers));

        if ($enabledDrivers === [] || array_diff($enabledDrivers, $known) !== [] || array_filter($enabledDrivers, 'is_string') !== $enabledDrivers) {
            throw new InvalidArgumentException('Enable at least one known transfer driver.');
        }

        if (! in_array($defaultDriver, $enabledDrivers, true)) {
            throw new InvalidArgumentException('The default transfer driver must be enabled.');
        }

        /** @var list<string> $enabledDrivers */
        return ['enabled_drivers' => $enabledDrivers, 'default_driver' => $defaultDriver];
    }
}
