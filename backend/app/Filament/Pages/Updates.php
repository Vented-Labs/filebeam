<?php

declare(strict_types=1);

namespace App\Filament\Pages;

use App\Models\User;
use App\Services\ReleaseChecker;
use Filament\Facades\Filament;
use Filament\Notifications\Notification;
use Filament\Pages\Page;

class Updates extends Page
{
    protected static ?string $title = 'Updates';

    protected static ?string $navigationLabel = 'Updates';

    protected static string|\BackedEnum|null $navigationIcon = 'filebeam-updates';

    protected static string|\UnitEnum|null $navigationGroup = 'Administration';

    protected static ?int $navigationSort = 60;

    protected string $view = 'filament.pages.updates';

    /** @var array<string, mixed> */
    public array $releaseCheck = [];

    /** @var array<string, mixed> */
    public array $updaterStatus = [];

    /** @var array<string, mixed> */
    public array $availability = [];

    /** @var array<string, mixed>|null */
    public ?array $updaterHeartbeat = null;

    public function mount(ReleaseChecker $releaseChecker): void
    {
        $this->refreshState($releaseChecker);
    }

    public static function canAccess(): bool
    {
        return Filament::auth()->user() instanceof User && Filament::auth()->user()->isAdmin();
    }

    public function checkNow(ReleaseChecker $releaseChecker): void
    {
        $releaseChecker->check();
        $this->refreshState($releaseChecker);

        Notification::make()->title('Release check complete')->success()->send();
    }

    public function upgradeNow(ReleaseChecker $releaseChecker): void
    {
        $user = Filament::auth()->user();
        abort_unless($user instanceof User && $user->isAdmin(), 403);
        $tag = $this->releaseCheck['latest']['tag'] ?? null;

        if (! is_string($tag)) {
            Notification::make()->title('No upgrade is available')->danger()->send();

            return;
        }

        try {
            $releaseChecker->queue($tag, (string) $user->getKey());
            $this->refreshState($releaseChecker);
            Notification::make()->title('Upgrade queued for the updater cron')->success()->send();
        } catch (\Throwable $exception) {
            Notification::make()->title('Upgrade was not queued')->body($exception->getMessage())->danger()->send();
        }
    }

    private function refreshState(ReleaseChecker $releaseChecker): void
    {
        $this->releaseCheck = $releaseChecker->state();
        $this->updaterStatus = $releaseChecker->updaterStatus();
        $this->updaterHeartbeat = $releaseChecker->updaterHeartbeat();
        $this->availability = $releaseChecker->availability();
    }
}
