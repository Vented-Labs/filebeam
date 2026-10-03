<?php

declare(strict_types=1);

namespace App\Enums;

enum ReceivingPolicy: string
{
    case Anyone = 'anyone';
    case Authenticated = 'authenticated';
    case Friends = 'friends';
    case Nobody = 'nobody';
}
