<?php

declare(strict_types=1);

use App\Models\Plan;
use App\Models\User;
use App\Support\EffectivePlan;
use Illuminate\Database\Eloquent\ModelNotFoundException;

test('default plan lookup is nullable for shared UI fallback while uploads remain fail closed', function (): void {
    $plans = app(EffectivePlan::class);

    expect($plans->default())->toBeNull()
        ->and(fn (): Plan => $plans->resolve(null))->toThrow(ModelNotFoundException::class);
});

test('an assigned plan remains effective even if it is inactive', function (): void {
    $assignedPlan = Plan::factory()->create(['is_active' => false]);
    $user = User::factory()->create(['plan_id' => $assignedPlan->id]);

    expect(app(EffectivePlan::class)->resolve($user)->is($assignedPlan))->toBeTrue();
});
