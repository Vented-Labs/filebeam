<?php

declare(strict_types=1);

namespace App\Support;

use App\Enums\Feature;

final class FeatureAvailability
{
    /** @param list<string>|null $extensions */
    public function __construct(private readonly ?array $extensions = null) {}

    /** @return list<string> */
    public function missing(Feature $feature): array
    {
        return array_values(array_diff($feature->extensions(), $this->extensions ?? get_loaded_extensions()));
    }

    public function available(Feature $feature): bool
    {
        return $this->missing($feature) === [];
    }

    /** @return array<string, array{available: bool, missing_extensions: list<string>}> */
    public function all(): array
    {
        $features = [];
        foreach (Feature::cases() as $feature) {
            $missing = $this->missing($feature);
            $features[$feature->value] = ['available' => $missing === [], 'missing_extensions' => $missing];
        }

        return $features;
    }
}
