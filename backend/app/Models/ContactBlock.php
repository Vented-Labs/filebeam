<?php

declare(strict_types=1);

namespace App\Models;

use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;

#[Fillable(['user_id', 'blocked_user_id'])]
class ContactBlock extends Model
{
    /** @param Builder<self> $query */
    #[Scope]
    protected function between(Builder $query, int $first, int $second): void
    {
        $query->where(fn (Builder $pair) => $pair->where('user_id', $first)->where('blocked_user_id', $second))
            ->orWhere(fn (Builder $pair) => $pair->where('user_id', $second)->where('blocked_user_id', $first));
    }
}
