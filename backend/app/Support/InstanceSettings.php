<?php

declare(strict_types=1);

namespace App\Support;

use App\Models\InstanceSetting;
use InvalidArgumentException;

class InstanceSettings
{
    /**
     * @return array<string, array{label: string, description: string, fallback: string, environment: string}>
     */
    public function definitions(): array
    {
        /** @var array<string, array{label: string, description: string, fallback: string, environment: string}> $definitions */
        $definitions = config('filebeam.instance_settings.definitions');

        return $definitions;
    }

    public function boolean(string $key): bool
    {
        return $this->booleans([$key])[$key];
    }

    /**
     * @param  list<string>  $keys
     * @return array<string, bool>
     */
    public function booleans(array $keys): array
    {
        /** @var array<string, array{label: string, description: string, fallback: string, environment: string}> $definitions */
        $definitions = [];
        /** @var array<string, bool> $values */
        $values = [];
        /** @var list<string> $databaseKeys */
        $databaseKeys = [];

        foreach ($keys as $key) {
            $definition = $this->definition($key);
            $definitions[$key] = $definition;
            $environmentValue = config('filebeam.instance_settings.environment.'.$definition['environment']);

            if ($environmentValue !== null) {
                $values[$key] = filter_var($environmentValue, FILTER_VALIDATE_BOOLEAN);
            } else {
                $databaseKeys[] = $key;
            }
        }

        /** @var array<string, bool> $databaseValues */
        $databaseValues = $databaseKeys === []
            ? []
            : InstanceSetting::query()->whereKey($databaseKeys)->pluck('value', 'key')->all();

        foreach ($databaseKeys as $key) {
            $values[$key] = $databaseValues[$key] ?? (bool) config($definitions[$key]['fallback']);
        }

        return $values;
    }

    public function environmentValue(string $key): ?bool
    {
        $environmentValue = config('filebeam.instance_settings.environment.'.$this->definition($key)['environment']);

        return $environmentValue === null ? null : filter_var($environmentValue, FILTER_VALIDATE_BOOLEAN);
    }

    /** @return array{label: string, description: string, fallback: string, environment: string} */
    public function definition(string $key): array
    {
        return $this->definitions()[$key] ?? throw new InvalidArgumentException("Unknown instance setting [{$key}].");
    }
}
