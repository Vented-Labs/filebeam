<?php

declare(strict_types=1);

namespace App\Policies;

use App\Models\Plan;
use App\Models\User;

class PlanPolicy
{
    public function viewAny(User $user): bool
    {
        return $user->isAdmin();
    }

    public function view(User $user, Plan $plan): bool
    {
        return $user->isAdmin();
    }

    public function create(User $user): bool
    {
        return false;
    }

    public function update(User $user, Plan $plan): bool
    {
        return $user->isAdmin();
    }

    public function delete(User $user, Plan $plan): bool
    {
        return false;
    }

    public function restore(User $user, Plan $plan): bool
    {
        return false;
    }

    public function forceDelete(User $user, Plan $plan): bool
    {
        return false;
    }
}
