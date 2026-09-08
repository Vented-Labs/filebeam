<?php

declare(strict_types=1);

namespace App\Policies;

use App\Models\FileReport;
use App\Models\User;

class FileReportPolicy
{
    public function viewAny(User $user): bool
    {
        return $user->isStaff();
    }

    public function view(User $user, FileReport $report): bool
    {
        return $user->isStaff();
    }

    public function create(User $user): bool
    {
        return false;
    }

    public function update(User $user, FileReport $report): bool
    {
        return false;
    }

    public function delete(User $user, FileReport $report): bool
    {
        return false;
    }

    public function restore(User $user, FileReport $report): bool
    {
        return false;
    }

    public function forceDelete(User $user, FileReport $report): bool
    {
        return false;
    }

    public function assign(User $user, FileReport $report): bool
    {
        return $user->isStaff();
    }

    public function review(User $user, FileReport $report): bool
    {
        return $user->isStaff();
    }

    public function takedown(User $user, FileReport $report): bool
    {
        return $user->isStaff() && $report->transfer_id !== null;
    }
}
