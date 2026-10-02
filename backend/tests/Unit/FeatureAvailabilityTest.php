<?php

declare(strict_types=1);

use App\Enums\Feature;
use App\Support\FeatureAvailability;

test('extension capabilities report their missing requirements independently', function () {
    $features = new FeatureAvailability(['sodium', 'redis']);
    expect($features->available(Feature::CustomThemes))->toBeFalse()
        ->and($features->missing(Feature::CustomThemes))->toBe(['gd'])
        ->and($features->available(Feature::RedisCache))->toBeTrue()
        ->and((new FeatureAvailability(['gd']))->all()['custom_themes'])->toBe(['available' => true, 'missing_extensions' => []]);
});
