<?php

declare(strict_types=1);

namespace App\Filament\Pages;

class Dashboard extends \Filament\Pages\Dashboard
{
    protected static bool $isDiscovered = false;

    protected static ?string $title = 'Work overview';

    protected static ?string $navigationLabel = 'Overview';

    public function getSubheading(): string
    {
        return 'Claim a report, continue an investigation, or check uploads waiting for cleanup.';
    }
}
