<?php

declare(strict_types=1);

namespace App\Support;

use App\Models\InstanceSetting;
use InvalidArgumentException;

/**
 * Resolves every admin-editable setting the same way: environment, then the instance_settings row, then the PHP fallback.
 */
class InstanceSettings
{
    /**
     * @return array<string, array{label: string, description: string, type: string, fallback: string}>
     */
    public function definitions(): array
    {
        /** @var array<string, array{label: string, description: string, type: string, fallback: string}> $definitions */
        $definitions = config('filebeam.instance_settings.definitions');

        return $definitions;
    }

    /** @return array{label: string, description: string, type: string, fallback: string} */
    public function definition(string $key): array
    {
        return $this->definitions()[$key] ?? throw new InvalidArgumentException("Unknown instance setting [{$key}].");
    }

    public function value(string $key): mixed
    {
        return $this->values([$key])[$key];
    }

    /**
     * Resolves several keys with at most one query.
     *
     * @param  list<string>  $keys
     * @return array<string, mixed>
     */
    public function values(array $keys): array
    {
        $values = [];
        $databaseKeys = [];

        foreach ($keys as $key) {
            $this->definition($key);
            $environmentValue = $this->environmentValue($key);

            if ($environmentValue !== null) {
                $values[$key] = $environmentValue;
            } else {
                $databaseKeys[] = $key;
            }
        }

        $stored = $databaseKeys === [] ? [] : $this->stored($databaseKeys);

        foreach ($databaseKeys as $key) {
            $values[$key] = $stored[$key] ?? config($this->definition($key)['fallback']);
        }

        return $values;
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
        return array_map(static fn (mixed $value): bool => filter_var($value, FILTER_VALIDATE_BOOLEAN), $this->values($keys));
    }

    /**
     * The normalized environment value, or null when the key is not set in the environment.
     */
    public function environmentValue(string $key): mixed
    {
        return config('filebeam.instance_settings.environment.'.$key);
    }

    /**
     * Database overrides only, without environment or fallback; missing keys are null.
     *
     * @param  list<string>  $keys
     * @return array<string, mixed>
     */
    public function stored(array $keys): array
    {
        $stored = array_fill_keys($keys, null);

        foreach (InstanceSetting::query()->whereKey($keys)->get(['key', 'value']) as $setting) {
            $stored[$setting->key] = $setting->value;
        }

        return $stored;
    }
}
