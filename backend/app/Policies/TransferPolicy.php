<?php

declare(strict_types=1);

namespace App\Policies;

use App\Models\Transfer;
use App\Models\User;

class TransferPolicy
{
    public function viewAny(User $user): bool
    {
        return $user->isStaff();
    }

    public function view(User $user, Transfer $transfer): bool
    {
        return $user->isStaff();
    }

    public function create(User $user): bool
    {
        return false;
    }

    public function update(User $user, Transfer $transfer): bool
    {
        return false;
    }

    public function delete(User $user, Transfer $transfer): bool
    {
        return false;
    }

    public function restore(User $user, Transfer $transfer): bool
    {
        return false;
    }

    public function forceDelete(User $user, Transfer $transfer): bool
    {
        return false;
    }

    public function takedown(User $user, Transfer $transfer): bool
    {
        return $user->isStaff();
    }

    public function retryCleanup(User $user, Transfer $transfer): bool
    {
        return $user->isStaff();
    }
}
