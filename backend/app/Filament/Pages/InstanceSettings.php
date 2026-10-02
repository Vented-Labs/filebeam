<?php

declare(strict_types=1);

namespace App\Filament\Pages;

use App\Actions\Admin\ManageInstanceSettings;
use App\Enums\SocialPlatform;
use App\Models\User;
use App\Support\InstanceSettings as InstanceSettingsResolver;
use App\Support\InstanceSettingValue;
use App\Support\LinkUrl;
use App\Support\SmtpSettings;
use BackedEnum;
use Filament\Facades\Filament;
use Filament\Forms\Components\Repeater;
use Filament\Forms\Components\Select;
use Filament\Forms\Components\TextInput;
use Filament\Forms\Components\Toggle;
use Filament\Notifications\Notification;
use Filament\Pages\Page;
use Filament\Schemas\Components\Section;
use Filament\Schemas\Schema;
use Illuminate\Support\Facades\DB;
use Illuminate\Validation\ValidationException;
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
        $keys = array_keys($settings->definitions());
        $stored = $settings->stored($keys);
        $data = [];

        foreach ($keys as $key) {
            $data[$key] = $settings->environmentValue($key) ?? $stored[$key];
        }
        // The transport selects are required, so show the effective fallback instead of an empty control.
        foreach (['enabled_drivers', 'default_driver'] as $key) {
            $data[$key] ??= config($settings->definition($key)['fallback']);
        }

        $data['smtp'] = app(SmtpSettings::class)->form();
        $this->form->fill($data);
    }

    public function form(Schema $schema): Schema
    {
        $settings = app(InstanceSettingsResolver::class);
        $locked = static fn (string $key): bool => $settings->environmentValue($key) !== null;
        $help = static fn (string $key, string $inherit): string => $locked($key) ? 'Source: environment; this setting is locked.' : $inherit;
        $flags = array_filter($settings->definitions(), static fn (array $definition): bool => $definition['type'] === 'boolean');

        return $schema
            ->components([
                Section::make('SMTP mail')
                    ->description(app(SmtpSettings::class)->managed()
                        ? 'Source: environment. Mail transport is locked by the operator.'
                        : 'Configure outgoing email. Disabling saved SMTP settings removes them and restores the default mail configuration.')
                    ->disabled(app(SmtpSettings::class)->managed())
                    ->schema([
                        Toggle::make('smtp.enabled')->label('Use saved SMTP settings'),
                        TextInput::make('smtp.host')->label('SMTP host')->maxLength(253),
                        TextInput::make('smtp.port')->label('SMTP port')->integer()->minValue(1)->maxValue(65535),
                        Select::make('smtp.security')->label('Connection security')
                            ->options(['starttls' => 'STARTTLS (required)', 'tls' => 'TLS (implicit)', 'none' => 'None'])->native(false),
                        TextInput::make('smtp.username')->label('SMTP username')->maxLength(1024)->autocomplete('off'),
                        TextInput::make('smtp.password')->label('SMTP password')->password()->maxLength(4096)
                            ->autocomplete('new-password')->helperText('Leave blank to keep the saved password.'),
                        Toggle::make('smtp.clear_password')->label('Clear saved SMTP password'),
                        TextInput::make('smtp.from_address')->label('Sender email address')->email()->maxLength(254)
                            ->disabled(config('smtp.from_address') !== null)
                            ->helperText(config('smtp.from_address') !== null ? 'Source: environment; this setting is locked.' : null),
                        TextInput::make('smtp.from_name')->label('Sender name')->maxLength(255)
                            ->disabled(config('smtp.from_name') !== null)
                            ->helperText(config('smtp.from_name') !== null ? 'Source: environment; this setting is locked.' : null),
                    ])->columns(['default' => 1, 'sm' => 2]),
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
                    }, array_keys($flags), $flags)),
                Section::make('Public transfer transport')
                    ->description('HTTP uses configured storage. WebRTC is storage-free and applies only to public file links and notes. Environment values always take precedence.')
                    ->schema([
                        Select::make('enabled_drivers')
                            ->label('Enabled drivers')
                            ->options(['http' => 'HTTP', 'webrtc' => 'WebRTC'])
                            ->multiple()
                            ->minItems(1)
                            ->required()
                            ->helperText($help('enabled_drivers', 'Source: database override or HTTP-only fallback.'))
                            ->disabled($locked('enabled_drivers'))
                            ->native(false),
                        Select::make('default_driver')
                            ->label('Default driver')
                            ->options(['http' => 'HTTP', 'webrtc' => 'WebRTC'])
                            ->required()
                            ->helperText($help('default_driver', 'Source: database override or HTTP-only fallback.'))
                            ->disabled($locked('default_driver'))
                            ->native(false),
                    ])->columns(['default' => 1, 'sm' => 2]),
                Section::make('Branding')
                    ->description('The copyright line, project link, and community links shown on public pages. Environment values always take precedence.')
                    ->schema([
                        TextInput::make('copyright_holder')
                            ->label('Copyright holder')
                            ->maxLength(InstanceSettingValue::MAX_TEXT_LENGTH)
                            ->placeholder((string) config('filebeam.branding.copyright_holder'))
                            ->helperText($help('copyright_holder', 'Leave empty to use the PHP fallback.'))
                            ->disabled($locked('copyright_holder')),
                        TextInput::make('copyright_year')
                            ->label('Copyright year')
                            ->integer()
                            ->minValue(1970)
                            ->maxValue(2100)
                            ->placeholder((string) config('filebeam.branding.copyright_year'))
                            ->helperText($help('copyright_year', 'Leave empty to use the PHP fallback.'))
                            ->disabled($locked('copyright_year')),
                        TextInput::make('copyright_url')
                            ->label('Copyright link')
                            ->url()
                            ->maxLength(LinkUrl::MAX_LENGTH)
                            ->placeholder('https://')
                            ->helperText($help('copyright_url', 'Optional. Turns the copyright holder into a link.'))
                            ->disabled($locked('copyright_url')),
                        TextInput::make('github_url')
                            ->label('GitHub link')
                            ->url()
                            ->maxLength(LinkUrl::MAX_LENGTH)
                            ->placeholder((string) config('filebeam.branding.github_url'))
                            ->helperText($help('github_url', 'Shown in the header navigation. Leave empty to use the PHP fallback.'))
                            ->disabled($locked('github_url')),
                        Repeater::make('community_links')
                            ->label('Community links')
                            ->helperText('Shown in this order. Each platform can be linked once.'.($locked('community_links') ? ' Source: environment; these links are locked.' : ''))
                            ->columnSpanFull()
                            ->schema([
                                Select::make('platform')
                                    ->label('Platform')
                                    ->options(SocialPlatform::options())
                                    ->required()
                                    ->distinct()
                                    ->native(false),
                                TextInput::make('url')
                                    ->label('URL')
                                    ->url()
                                    ->required()
                                    ->maxLength(LinkUrl::MAX_LENGTH)
                                    ->placeholder('https://'),
                            ])
                            ->columns(['default' => 1, 'sm' => 2])
                            ->addActionLabel('Add link')
                            ->defaultItems(0)
                            ->maxItems(count(SocialPlatform::cases()))
                            ->disabled($locked('community_links')),
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
        $settings = app(InstanceSettingsResolver::class);
        $values = [];

        foreach (array_keys($settings->definitions()) as $key) {
            if ($settings->environmentValue($key) !== null) {
                continue;
            }

            $values[$key] = $key === 'community_links'
                ? array_values(is_array($state[$key] ?? null) ? $state[$key] : [])
                : ($state[$key] ?? null);
        }

        DB::transaction(function () use ($actor, $values, $state): void {
            app(ManageInstanceSettings::class)->update($actor, $values);
            if (! app(SmtpSettings::class)->managed()) {
                try {
                    app(SmtpSettings::class)->update($actor, $state['smtp']);
                } catch (ValidationException $exception) {
                    throw ValidationException::withMessages(collect($exception->errors())
                        ->mapWithKeys(static fn (array $messages, string $key): array => ['data.'.$key => $messages])->all());
                }
            }
        });
        $this->fillForm();

        Notification::make()->title('Instance settings updated')->success()->send();
    }
}
