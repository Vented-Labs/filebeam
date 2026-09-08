<?php

declare(strict_types=1);

use App\Providers\AdminServiceProvider;
use App\Providers\AppServiceProvider;
use App\Providers\Filament\AdminPanelProvider;

return [
    AppServiceProvider::class,
    AdminServiceProvider::class,
    AdminPanelProvider::class,
];
