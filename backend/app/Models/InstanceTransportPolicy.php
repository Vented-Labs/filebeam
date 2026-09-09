<?php

declare(strict_types=1);

namespace App\Models;

use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Model;

#[Fillable(['enabled_drivers', 'default_driver'])]
class InstanceTransportPolicy extends Model
{
    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'enabled_drivers' => 'array',
        ];
    }
}
