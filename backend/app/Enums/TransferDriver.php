<?php

declare(strict_types=1);

namespace App\Enums;

enum TransferDriver: string
{
    case Http = 'http';
    case WebRtc = 'webrtc';
}
