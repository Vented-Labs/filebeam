<?php

declare(strict_types=1);

namespace App\Filament\Resources\AdminAudits\Pages;

use App\Filament\Resources\AdminAudits\AdminAuditResource;
use Filament\Resources\Pages\ListRecords;

class ListAdminAudits extends ListRecords
{
    protected static string $resource = AdminAuditResource::class;
}
