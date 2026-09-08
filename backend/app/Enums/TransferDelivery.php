<?php

declare(strict_types=1);

namespace App\Enums;

enum TransferDelivery: string
{
    case Link = 'link';
    case Inbox = 'inbox';
}
