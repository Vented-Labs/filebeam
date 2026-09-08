<?php

declare(strict_types=1);

namespace App\Policies;

use App\Models\Filestore;
use App\Models\User;
use App\Support\FilestoreRegistry;

class FilestorePolicy
{
    public function viewAny(User $user): bool
    {
        return $user->isAdmin();
    }

    public function view(User $user, Filestore $filestore): bool
    {
        return $user->isAdmin();
    }

    public function create(User $user): bool
    {
        return $user->isAdmin() && ! app(FilestoreRegistry::class)->environmentManaged();
    }

    public function update(User $user, Filestore $filestore): bool
    {
        return $user->isAdmin()
            && ! app(FilestoreRegistry::class)->environmentManaged();
    }

    public function delete(User $user, Filestore $filestore): bool
    {
        return $this->update($user, $filestore);
    }

    public function restore(User $user, Filestore $filestore): bool
    {
        return false;
    }

    public function forceDelete(User $user, Filestore $filestore): bool
    {
        return false;
    }
}
