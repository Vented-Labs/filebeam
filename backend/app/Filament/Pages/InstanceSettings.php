<?php

declare(strict_types=1);

namespace App\Filament\Pages;

use App\Actions\Admin\ManageInstanceSettings;
use App\Models\InstanceSetting;
use App\Models\User;
use App\Support\InstanceSettings as InstanceSettingsResolver;
use BackedEnum;
use Filament\Facades\Filament;
use Filament\Forms\Components\Select;
use Filament\Notifications\Notification;
use Filament\Pages\Page;
use Filament\Schemas\Components\Section;
use Filament\Schemas\Schema;
use Throwable;
use UnitEnum;

/** @property-read Schema $form */
class InstanceSettings extends Page
{
    protected static ?string $title = 'Instance settings';

    protected static ?string $navigationLabel = 'Instance settings';

    protected static string|BackedEnum|null $navigationIcon = 'filebeam-settings';

    protected static string|UnitEnum|null $navigationGroup = 'Administration';

    protected static ?int $navigationSort = 50;

    /** @var array<string, mixed> */
    public array $data = [];

    protected string $view = 'filament.pages.instance-settings';

    public static function canAccess(): bool
    {
        return Filament::auth()->user() instanceof User && Filament::auth()->user()->isAdmin();
    }

    public function mount(): void
    {
        $this->fillForm();
    }

    private function fillForm(): void
    {
        $settings = app(InstanceSettingsResolver::class);
        $overrides = InstanceSetting::query()->pluck('value', 'key')->all();
        $data = [];

        foreach (array_keys($settings->definitions()) as $key) {
            $data[$key] = $settings->environmentValue($key) ?? ($overrides[$key] ?? null);
        }

        $this->form->fill($data);
    }

    public function form(Schema $schema): Schema
    {
        $settings = app(InstanceSettingsResolver::class);

        return $schema
            ->components([
                Section::make('Feature availability')
                    ->description('Choose Inherit to use the PHP fallback. Environment values always take precedence.')
                    ->schema(array_map(function (string $key, array $definition) use ($settings): Select {
                        $environmentValue = $settings->environmentValue($key);

                        return Select::make($key)
                            ->label($definition['label'])
                            ->options(['1' => 'Enabled', '0' => 'Disabled'])
                            ->placeholder('Inherit (PHP fallback)')
                            ->helperText($environmentValue === null
                                ? $definition['description'].' Source: database override or PHP fallback.'
                                : $definition['description'].' Source: environment ('.($environmentValue ? 'Enabled' : 'Disabled').'); this setting is locked.')
                            ->disabled($environmentValue !== null)
                            ->native(false);
                    }, array_keys($settings->definitions()), $settings->definitions())),
            ])
            ->statePath('data');
    }

    /**
     * @throws Throwable
     */
    public function save(): void
    {
        $actor = Filament::auth()->user();
        abort_unless($actor instanceof User, 403);

        app(ManageInstanceSettings::class)->update($actor, $this->form->getState());
        $this->fillForm();

        Notification::make()->title('Instance settings updated')->success()->send();
    }
}
