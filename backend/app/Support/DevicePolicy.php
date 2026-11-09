<?php

declare(strict_types=1);

namespace App\Support;

class DevicePolicy
{
    public function activeLimit(): int
    {
        return $this->integer('device_active_limit');
    }

    public function connectionTtlSeconds(): int
    {
        return $this->integer('device_connection_ttl_seconds');
    }

    public function presenceTtlSeconds(): int
    {
        return $this->integer('device_presence_ttl_seconds');
    }

    public function offlineFreshnessSeconds(): int
    {
        return $this->integer('device_offline_freshness_seconds');
    }

    public function requestsPerMinute(): int
    {
        return $this->integer('device_requests_per_minute');
    }

    private function integer(string $key): int
    {
        $value = app(InstanceSettings::class)->value($key);
        assert(is_int($value));

        return $value;
    }
}
