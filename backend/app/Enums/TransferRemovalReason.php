<?php

declare(strict_types=1);

namespace App\Enums;

enum TransferRemovalReason: string
{
    case Deleted = 'deleted';
    case Expired = 'expired';
    case Abandoned = 'abandoned';
    case Burned = 'burned';
    case Removed = 'removed';
}
