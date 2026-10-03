<?php

declare(strict_types=1);

namespace App\Models;

use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;

#[Fillable(['lower_user_id', 'upper_user_id', 'requester_id', 'accepted_at'])]
class Friendship extends Model
{
    /** @param Builder<self> $query */
    #[Scope]
    protected function between(Builder $query, int $first, int $second): void
    {
        $query->where('lower_user_id', min($first, $second))->where('upper_user_id', max($first, $second));
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function involving(Builder $query, int $userId): void
    {
        $query->where(fn (Builder $pair) => $pair->where('lower_user_id', $userId)->orWhere('upper_user_id', $userId));
    }

    /** @return HasMany<ContactPreference, $this> */
    public function preferences(): HasMany
    {
        return $this->hasMany(ContactPreference::class);
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return ['accepted_at' => 'immutable_datetime'];
    }
}
