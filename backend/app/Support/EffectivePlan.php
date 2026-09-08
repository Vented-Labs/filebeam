<?php

declare(strict_types=1);

namespace App\Support;

use App\Models\Plan;
use App\Models\User;
use Illuminate\Database\Eloquent\ModelNotFoundException;

class EffectivePlan
{
    public function resolve(?User $user): Plan
    {
        return $user->plan ?? $this->default() ?? throw (new ModelNotFoundException)->setModel(Plan::class);
    }

    public function default(): ?Plan
    {
        return Plan::query()
            ->default(config('filebeam.transfers.default_plan'))
            ->active()
            ->first();
    }
}
