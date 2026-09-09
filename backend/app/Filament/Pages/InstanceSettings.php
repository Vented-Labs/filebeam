<?php

declare(strict_types=1);

namespace App\Filament\Pages;

use App\Actions\Admin\ManageInstanceSettings;
use App\Actions\Admin\ManageTransportPolicy;
use App\Models\InstanceSetting;
use App\Models\InstanceTransportPolicy;
use App\Models\User;
use App\Support\InstanceSettings as InstanceSettingsResolver;
use BackedEnum;
use Filament\Facades\Filament;
use Filament\Forms\Components\Select;
use Filament\Notifications\Notification;
use Filament\Pages\Page;
use Filament\Schemas\Components\Section;
use Filament\Schemas\Schema;
use Illuminate\Support\Facades\DB;
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
        $policy = InstanceTransportPolicy::query()->find(1);
        $environment = config('filebeam.transport_policy.environment');
        $data['enabled_drivers'] = $environment['enabled_drivers'] ?? ($policy === null ? null : $policy->enabled_drivers) ?? config('filebeam.transport_policy.defaults.enabled_drivers');
        $data['default_driver'] = $environment['default_driver'] ?? ($policy === null ? null : $policy->default_driver) ?? config('filebeam.transport_policy.defaults.default_driver');

        $this->form->fill($data);
    }

    public function form(Schema $schema): Schema
    {
        $settings = app(InstanceSettingsResolver::class);
        $environment = config('filebeam.transport_policy.environment');

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
                Section::make('Public transfer transport')
                    ->description('HTTP uses configured storage. WebRTC is storage-free and applies only to public file links and notes. Environment values always take precedence.')
                    ->schema([
                        Select::make('enabled_drivers')
                            ->label('Enabled drivers')
                            ->options(['http' => 'HTTP', 'webrtc' => 'WebRTC'])
                            ->multiple()
                            ->minItems(1)
                            ->required()
                            ->helperText($environment['enabled_drivers'] === null ? 'Source: database override or HTTP-only fallback.' : 'Source: environment; this setting is locked.')
                            ->disabled($environment['enabled_drivers'] !== null)
                            ->native(false),
                        Select::make('default_driver')
                            ->label('Default driver')
                            ->options(['http' => 'HTTP', 'webrtc' => 'WebRTC'])
                            ->required()
                            ->helperText($environment['default_driver'] === null ? 'Source: database override or HTTP-only fallback.' : 'Source: environment; this setting is locked.')
                            ->disabled($environment['default_driver'] !== null)
                            ->native(false),
                    ])->columns(['default' => 1, 'sm' => 2]),
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

        $state = $this->form->getState();
        $transportValues = [];
        $environment = config('filebeam.transport_policy.environment');
        foreach (['enabled_drivers', 'default_driver'] as $key) {
            if ($environment[$key] === null) {
                $transportValues[$key] = $state[$key];
            }
            unset($state[$key]);
        }

        DB::transaction(function () use ($actor, $state, $transportValues): void {
            app(ManageInstanceSettings::class)->update($actor, $state);
            app(ManageTransportPolicy::class)->update($actor, $transportValues);
        });
        $this->fillForm();

        Notification::make()->title('Instance settings updated')->success()->send();
    }
}
