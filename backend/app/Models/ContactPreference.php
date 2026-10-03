<?php

declare(strict_types=1);

namespace App\Models;

use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Model;

#[Fillable(['friendship_id', 'user_id', 'can_send', 'auto_download'])]
class ContactPreference extends Model
{
    /** @return array<string, string> */
    protected function casts(): array
    {
        return ['can_send' => 'boolean', 'auto_download' => 'boolean'];
    }
}
