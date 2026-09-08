<?php

declare(strict_types=1);

namespace App\Filament\Resources\Plans;

use App\Filament\Resources\Plans\Pages\EditPlan;
use App\Filament\Resources\Plans\Pages\ListPlans;
use App\Filament\Resources\Plans\Pages\ViewPlan;
use App\Filament\Resources\Plans\RelationManagers\AssignedUsersRelationManager;
use App\Models\Filestore;
use App\Models\Plan;
use Filament\Actions\Action;
use Filament\Forms\Components\Placeholder;
use Filament\Forms\Components\Select;
use Filament\Forms\Components\TextInput;
use Filament\Forms\Components\Toggle;
use Filament\Infolists\Components\TextEntry;
use Filament\Resources\Pages\PageRegistration;
use Filament\Resources\Resource;
use Filament\Schemas\Components\Group;
use Filament\Schemas\Components\Section;
use Filament\Schemas\Components\Utilities\Get;
use Filament\Schemas\Schema;
use Filament\Support\Icons\Heroicon;
use Filament\Tables\Columns\IconColumn;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;

class PlanResource extends Resource
{
    protected static ?string $model = Plan::class;

    protected static ?string $recordTitleAttribute = 'name';

    protected static string|\BackedEnum|null $navigationIcon = Heroicon::OutlinedRectangleStack;

    protected static string|\UnitEnum|null $navigationGroup = 'Administration';

    protected static ?int $navigationSort = 40;

    public static function table(Table $table): Table
    {
        return $table
            ->defaultSort('name')
            ->recordUrl(fn (Plan $record): string => static::getUrl('view', ['record' => $record]))
            ->columns([
                TextColumn::make('name')->sortable(),
                TextColumn::make('slug'),
                TextColumn::make('maximum_transfer_bytes')->label('Transfer limit')->state(fn (Plan $record): string => self::formatBytes($record->maximum_transfer_bytes)),
                TextColumn::make('maximum_file_count')->label('File limit')->numeric(),
                TextColumn::make('default_file_retention_hours')->label('File retention')->state(fn (Plan $record): string => self::formatHours($record->default_file_retention_hours)),
                TextColumn::make('default_note_retention_hours')->label('Note retention')->state(fn (Plan $record): string => self::formatHours($record->default_note_retention_hours)),
                IconColumn::make('is_active')->label('Active')->boolean(),
                TextColumn::make('placement_mode')->label('Placement')->badge(),
            ])
            ->recordActions([
                Action::make('view')->url(fn (Plan $record): string => static::getUrl('view', ['record' => $record])),
                Action::make('editLimits')->label('Edit limits')->authorize('update')->url(fn (Plan $record): string => static::getUrl('edit', ['record' => $record])),
            ]);
    }

    public static function infolist(Schema $schema): Schema
    {
        return $schema->schema([
            Section::make('Plan details')
                ->columnSpanFull()
                ->schema([
                    TextEntry::make('name'),
                    TextEntry::make('slug'),
                    TextEntry::make('is_active')->label('Availability')->state(fn (Plan $record): string => $record->is_active ? 'Available for assignment' : 'Unavailable for assignment')->badge(),
                ])->columns(['default' => 1, 'sm' => 3]),
            Section::make('File transfers')
                ->columnSpanFull()
                ->schema([
                    TextEntry::make('maximum_transfer_bytes')->label('Maximum transfer size')->state(fn (Plan $record): string => self::formatBytes($record->maximum_transfer_bytes)),
                    TextEntry::make('maximum_file_count')->label('Maximum files')->numeric(),
                ])->columns(['default' => 1, 'sm' => 2]),
            Section::make('Notes')
                ->columnSpanFull()
                ->schema([
                    TextEntry::make('maximum_note_bytes')->label('Maximum note size')->state(fn (Plan $record): string => self::formatBytes($record->maximum_note_bytes)),
                ]),
            Section::make('Retention')
                ->columnSpanFull()
                ->schema([
                    TextEntry::make('default_file_retention_hours')->label('Default file retention')->state(fn (Plan $record): string => self::formatHours($record->default_file_retention_hours)),
                    TextEntry::make('maximum_file_retention_hours')->label('Maximum file retention')->state(fn (Plan $record): string => self::formatHours($record->maximum_file_retention_hours)),
                    TextEntry::make('default_note_retention_hours')->label('Default note retention')->state(fn (Plan $record): string => self::formatHours($record->default_note_retention_hours)),
                    TextEntry::make('maximum_note_retention_hours')->label('Maximum note retention')->state(fn (Plan $record): string => self::formatHours($record->maximum_note_retention_hours)),
                ])->columns(['default' => 1, 'sm' => 2]),
            Section::make('Storage placement')
                ->columnSpanFull()
                ->schema([
                    TextEntry::make('placement_mode')->label('Mode')->badge(),
                    TextEntry::make('filestores')->label('Allowed stores')->state(fn (Plan $record): string => $record->filestores()->orderBy('name')->pluck('name')->join(', '))->placeholder('No stores assigned'),
                    TextEntry::make('default_filestores')->label('Default stores')->state(fn (Plan $record): string => $record->filestores()->wherePivot('is_default', true)->orderBy('name')->pluck('name')->join(', '))->placeholder('No default stores assigned'),
                ]),
        ]);
    }

    /** @return array<string, PageRegistration> */
    public static function getPages(): array
    {
        return [
            'index' => ListPlans::route('/'),
            'view' => ViewPlan::route('/{record}'),
            'edit' => EditPlan::route('/{record}/edit'),
        ];
    }

    /** @return array<class-string> */
    public static function getRelations(): array
    {
        return [AssignedUsersRelationManager::class];
    }

    public static function form(Schema $schema): Schema
    {
        return $schema->schema([
            Section::make('File transfers')
                ->columnSpanFull()
                ->description('Changes apply to relevant future transfers. Existing transfer snapshots are unchanged.')
                ->schema([
                    self::sizedInput('maximum_transfer', 'Maximum transfer size'),
                    self::positiveInteger('maximum_file_count', 'Maximum file count', 32767),
                    self::settingSummary('transfer', 'maximum_transfer', 'B', fn (Plan $record): string => self::formatBytes($record->maximum_transfer_bytes)),
                ])->columns(1),
            Section::make('Notes')
                ->columnSpanFull()
                ->description('Changes apply to relevant future transfers. Existing transfer snapshots are unchanged.')
                ->schema([
                    self::sizedInput('maximum_note', 'Maximum note size'),
                    self::settingSummary('note', 'maximum_note', 'B', fn (Plan $record): string => self::formatBytes($record->maximum_note_bytes)),
                ])->columns(1),
            Section::make('Retention')
                ->columnSpanFull()
                ->description('Changes apply to relevant future transfers. Existing transfer snapshots are unchanged.')
                ->schema([
                    Section::make('Files')
                        ->schema([
                            self::durationInput('default_file_retention', 'Default file retention'),
                            self::durationInput('maximum_file_retention', 'Maximum file retention'),
                            self::retentionSummary('file', fn (Plan $record): string => sprintf('Default %s; maximum %s', self::formatHours($record->default_file_retention_hours), self::formatHours($record->maximum_file_retention_hours))),
                        ])->columns(1),
                    Section::make('Notes')
                        ->schema([
                            self::durationInput('default_note_retention', 'Default note retention'),
                            self::durationInput('maximum_note_retention', 'Maximum note retention'),
                            self::retentionSummary('note', fn (Plan $record): string => sprintf('Default %s; maximum %s', self::formatHours($record->default_note_retention_hours), self::formatHours($record->maximum_note_retention_hours))),
                        ])->columns(1),
                ])->columns(['default' => 1, 'xl' => 2]),
            Section::make('Availability')
                ->columnSpanFull()
                ->schema([
                    Toggle::make('is_active')
                        ->label('Available for assignment')
                        ->helperText(fn (Plan $record): string => $record->slug === config('filebeam.transfers.default_plan') ? 'This configured default plan must remain available so accounts without an explicit plan can create transfers.' : 'Inactive plans cannot be assigned to accounts; existing transfer snapshots are unchanged.')
                        ->required(),
                ]),
            Section::make('Storage placement')
                ->columnSpanFull()
                ->description('New transfers use this allowed subset. Existing transfer snapshots are unchanged.')
                ->schema([
                    Select::make('placement_mode')->options(['distribute' => 'Distribute (default)', 'replicate' => 'Replicate'])->required(),
                    Select::make('filestore_ids')->label('Allowed stores')->multiple()->options(fn (): array => Filestore::query()->orderBy('name')->pluck('name', 'id')->all()),
                    Select::make('default_filestore_ids')->label('Default stores')->multiple()->helperText('Uploads use this default pool. Every default must be in the allowed store subset and available for placement.')->options(fn (): array => Filestore::query()->orderBy('name')->pluck('name', 'id')->all()),
                ]),
        ]);
    }

    public static function positiveInteger(string $name, string $label, int $maximum): TextInput
    {
        return TextInput::make($name)->label($label)->numeric()->integer()->minValue(1)->maxValue($maximum)->required();
    }

    /** @return array<string, string> */
    public static function units(): array
    {
        return ['B' => 'bytes (B)', 'KiB' => 'KiB', 'MiB' => 'MiB', 'GiB' => 'GiB'];
    }

    /** @return array<string, string> */
    public static function durationUnits(): array
    {
        return ['hours' => 'hours', 'days' => 'days'];
    }

    public static function byteMultiplier(string $unit): int
    {
        return match ($unit) {
            'GiB' => 1024 * 1024 * 1024,
            'MiB' => 1024 * 1024,
            'KiB' => 1024,
            default => 1,
        };
    }

    public static function durationMultiplier(string $unit): int
    {
        return $unit === 'days' ? 24 : 1;
    }

    /** @return array{quantity: int, unit: string} */
    public static function humanBytes(int $bytes): array
    {
        foreach (['GiB', 'MiB', 'KiB'] as $unit) {
            $multiplier = self::byteMultiplier($unit);

            if ($bytes % $multiplier === 0) {
                return ['quantity' => intdiv($bytes, $multiplier), 'unit' => $unit];
            }
        }

        return ['quantity' => $bytes, 'unit' => 'B'];
    }

    /** @return array{quantity: int, unit: string} */
    public static function humanHours(int $hours): array
    {
        if ($hours % 24 === 0) {
            return ['quantity' => intdiv($hours, 24), 'unit' => 'days'];
        }

        return ['quantity' => $hours, 'unit' => 'hours'];
    }

    public static function formatBytes(int $bytes): string
    {
        $human = self::humanBytes($bytes);

        return number_format($human['quantity']).' '.$human['unit'];
    }

    public static function formatHours(int $hours): string
    {
        $human = self::humanHours($hours);

        return self::selectedValue($human['quantity'], $human['unit'], 'hours');
    }

    public static function selectedValue(mixed $quantity, mixed $unit, string $fallbackUnit): string
    {
        $quantity = (int) ($quantity ?? 0);
        $unit = $unit ?: $fallbackUnit;

        if ($quantity === 1 && in_array($unit, ['hours', 'days'], true)) {
            $unit = substr($unit, 0, -1);
        }

        return number_format($quantity).' '.$unit;
    }

    private static function sizedInput(string $name, string $label): Group
    {
        return Group::make([
            TextInput::make("{$name}_quantity")->label($label)->numeric()->integer()->minValue(1)->required()->live(),
            Select::make("{$name}_unit")->label('Unit')->options(self::units())->required()->live(),
        ])->columns(2);
    }

    private static function durationInput(string $name, string $label): Group
    {
        return Group::make([
            TextInput::make("{$name}_quantity")->label($label)->numeric()->integer()->minValue(1)->required()->live(),
            Select::make("{$name}_unit")->label('Unit')->options(self::durationUnits())->required()->live(),
        ])->columns(2);
    }

    /** @param callable(Plan): string $current */
    private static function settingSummary(string $name, string $prefix, string $fallbackUnit, callable $current): Group
    {
        return Group::make([
            Placeholder::make("{$name}_current")->label('Current setting')->content($current),
            Placeholder::make("{$name}_proposed")->label('Proposed setting')->content(fn (Get $get): string => self::selectedValue($get("{$prefix}_quantity"), $get("{$prefix}_unit"), $fallbackUnit)),
        ])->columns(['default' => 1, 'sm' => 2]);
    }

    /** @param callable(Plan): string $current */
    private static function retentionSummary(string $name, callable $current): Group
    {
        return Group::make([
            Placeholder::make("{$name}_retention_current")->label('Current setting')->content($current),
            Placeholder::make("{$name}_retention_proposed")->label('Proposed setting')->content(fn (Get $get): string => sprintf('Default %s; maximum %s', self::selectedValue($get("default_{$name}_retention_quantity"), $get("default_{$name}_retention_unit"), 'hours'), self::selectedValue($get("maximum_{$name}_retention_quantity"), $get("maximum_{$name}_retention_unit"), 'hours'))),
        ])->columns(['default' => 1, 'sm' => 2]);
    }
}
