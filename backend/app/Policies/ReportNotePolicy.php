<?php

declare(strict_types=1);

namespace App\Policies;

use App\Models\ReportNote;
use App\Models\User;

class ReportNotePolicy
{
    public function viewAny(User $user): bool
    {
        return $user->isStaff();
    }

    public function view(User $user, ReportNote $note): bool
    {
        return $user->isStaff();
    }

    public function create(User $user): bool
    {
        return $user->isStaff();
    }

    public function update(User $user, ReportNote $note): bool
    {
        return false;
    }

    public function delete(User $user, ReportNote $note): bool
    {
        return false;
    }
}
