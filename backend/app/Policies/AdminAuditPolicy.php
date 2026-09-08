<?php

declare(strict_types=1);

namespace App\Policies;

use App\Models\AdminAudit;
use App\Models\User;

class AdminAuditPolicy
{
    public function viewAny(User $user): bool
    {
        return $user->isAdmin();
    }

    public function view(User $user, AdminAudit $audit): bool
    {
        return $user->isAdmin();
    }

    public function create(User $user): bool
    {
        return false;
    }

    public function update(User $user, AdminAudit $audit): bool
    {
        return false;
    }

    public function delete(User $user, AdminAudit $audit): bool
    {
        return false;
    }

    public function restore(User $user, AdminAudit $audit): bool
    {
        return false;
    }

    public function forceDelete(User $user, AdminAudit $audit): bool
    {
        return false;
    }
}
