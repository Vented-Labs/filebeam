<?php

declare(strict_types=1);

namespace App\Services\Changelog;

use RuntimeException;

final class ChangelogSyncException extends RuntimeException
{
    public function __construct(public readonly string $reason, public readonly ?int $retryAfter = null)
    {
        parent::__construct('Changelog synchronization failed: '.$reason);
    }
}
