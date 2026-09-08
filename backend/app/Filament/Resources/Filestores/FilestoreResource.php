<?php

declare(strict_types=1);

namespace App\Filament\Resources\Filestores;

use App\Filament\Resources\Filestores\Pages\CreateFilestore;
use App\Filament\Resources\Filestores\Pages\EditFilestore;
use App\Filament\Resources\Filestores\Pages\ListFilestores;
use App\Filament\Resources\Filestores\Pages\ViewFilestore;
use App\Models\Filestore;
use App\Models\Plan;
use App\Support\FilestoreRegistry;
use Filament\Forms\Components\Select;
use Filament\Forms\Components\TextInput;
use Filament\Forms\Components\Toggle;
use Filament\Infolists\Components\TextEntry;
use Filament\Resources\Pages\PageRegistration;
use Filament\Resources\Resource;
use Filament\Schemas\Components\Section;
use Filament\Schemas\Schema;
use Filament\Support\Icons\Heroicon;
use Filament\Tables\Columns\IconColumn;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Support\Facades\DB;

class FilestoreResource extends Resource
{
    protected static ?string $model = Filestore::class;

    protected static ?string $recordTitleAttribute = 'name';

    protected static string|\BackedEnum|null $navigationIcon = Heroicon::OutlinedCircleStack;

    protected static string|\UnitEnum|null $navigationGroup = 'Administration';

    protected static ?int $navigationSort = 45;

    public static function getEloquentQuery(): Builder
    {
        return parent::getEloquentQuery()
            ->withCount(['locations', 'uploadAttempts'])
            ->withSum('locations as physical_bytes', 'ciphertext_bytes')
            ->withSum('uploadAttempts as attempt_bytes', 'ciphertext_bytes')
            ->selectSub(self::fileAggregate(), 'file_items_count')
            ->selectSub(self::noteAggregate(), 'note_items_count')
            ->selectSub(
                DB::table('plan_filestore')
                    ->selectRaw('count(*)')
                    ->whereColumn('plan_filestore.filestore_id', 'filestores.id'),
                'assigned_plans_count',
            );
    }

    public static function table(Table $table): Table
    {
        return $table->defaultSort('name')->recordUrl(fn (Filestore $record): string => static::getUrl('view', ['record' => $record]))->columns([
            TextColumn::make('name')->searchable()->sortable(),
            TextColumn::make('storage_driver')->label('Driver')->state(fn (Filestore $record): string => self::driver($record)),
            TextColumn::make('source')->badge(),
            IconColumn::make('effective_placement')->label('Placement')->state(fn (Filestore $record): bool => app(FilestoreRegistry::class)->placementEnabled($record))->boolean(),
            TextColumn::make('file_items_count')->label('Files')->numeric(),
            TextColumn::make('note_items_count')->label('Notes')->numeric(),
            TextColumn::make('locations_count')->label('Physical chunks')->numeric(),
            TextColumn::make('physical_bytes')->label('Chunk bytes')->numeric(),
            TextColumn::make('upload_attempts_count')->label('Attempts')->numeric(),
            TextColumn::make('attempt_bytes')->label('Attempt bytes')->numeric(),
            TextColumn::make('assigned_plans_count')->label('Assigned plans')->numeric(),
        ]);
    }

    public static function infolist(Schema $schema): Schema
    {
        return $schema->schema([
            Section::make('Store')->schema([
                TextEntry::make('name'), TextEntry::make('source')->badge(), TextEntry::make('storage_driver')->label('Driver')->state(fn (Filestore $record): string => self::driver($record)), TextEntry::make('disk_name')->placeholder('Managed configuration'), TextEntry::make('effective_placement')->label('Placement enabled')->state(fn (Filestore $record): string => app(FilestoreRegistry::class)->placementEnabled($record) ? 'Enabled' : 'Disabled')->badge(),
            ])->columns(['default' => 1, 'sm' => 3]),
            Section::make('Usage')->description('Counts are database records only; object storage is never scanned.')->schema([
                TextEntry::make('file_items_count')->label('Files')->numeric(),
                TextEntry::make('note_items_count')->label('Notes')->numeric(),
                TextEntry::make('locations_count')->label('Physical chunks')->numeric(),
                TextEntry::make('physical_bytes')->label('Physical bytes')->numeric(),
                TextEntry::make('upload_attempts_count')->label('Upload attempts')->numeric(),
                TextEntry::make('attempt_bytes')->label('Attempt bytes')->numeric(),
                TextEntry::make('assigned_plans')->label('Assigned plans')->state(fn (Filestore $record): array => self::assignedPlanLabels($record))->badge()->listWithLineBreaks()->placeholder('None'),
            ])->columns(['default' => 1, 'sm' => 3]),
        ]);
    }

    public static function form(Schema $schema): Schema
    {
        return $schema->schema([
            Section::make('Store')->schema([
                TextInput::make('name')->required()->maxLength(255),
                Select::make('source')->options(fn (string $operation): array => $operation === 'edit' ? ['database' => 'Managed database store', 'laravel' => 'Installed Laravel disk', 'environment' => 'Environment-managed disk (historical)'] : ['database' => 'Managed database store', 'laravel' => 'Installed Laravel disk'])->required()->live()->disabledOn('edit')->dehydrated(fn (string $operation): bool => $operation === 'create'),
                Select::make('disk_name')->label('Laravel disk')->options(fn (): array => array_combine(array_keys(config('filesystems.disks', [])), array_keys(config('filesystems.disks', []))) ?: [])->visible(fn ($get): bool => $get('source') === 'laravel')->required(fn ($get): bool => $get('source') === 'laravel')->disabledOn('edit'),
                Select::make('driver')->options(['local' => 'Local directory', 's3' => 'S3-compatible'])->live()->visible(fn ($get): bool => $get('source') === 'database')->required(fn ($get): bool => $get('source') === 'database')->disabledOn('edit')->dehydrated(fn (string $operation): bool => $operation === 'create'),
                Toggle::make('placement_enabled')->label('Enable new placement')->required(),
            ])->columns(['default' => 1, 'sm' => 2]),
            Section::make('Managed storage configuration')->visible(fn ($get): bool => $get('source') === 'database')->schema([
                TextInput::make('configuration.root')->label('Relative local directory')->visible(fn ($get): bool => $get('driver') === 'local')->disabledOn('edit')->dehydrated(),
                TextInput::make('configuration.bucket')->visible(fn ($get): bool => $get('driver') === 's3')->disabledOn('edit')->dehydrated(),
                TextInput::make('configuration.region')->visible(fn ($get): bool => $get('driver') === 's3')->disabledOn('edit')->dehydrated(),
                TextInput::make('configuration.endpoint')->url()->visible(fn ($get): bool => $get('driver') === 's3')->disabledOn('edit')->dehydrated(),
                Toggle::make('configuration.use_path_style_endpoint')->visible(fn ($get): bool => $get('driver') === 's3'),
                TextInput::make('configuration.key')->password()->revealable()->visible(fn ($get): bool => $get('driver') === 's3')->dehydrated(fn ($state): bool => filled($state)),
                TextInput::make('configuration.secret')->password()->revealable()->visible(fn ($get): bool => $get('driver') === 's3')->dehydrated(fn ($state): bool => filled($state)),
            ])->columns(['default' => 1, 'sm' => 2]),
        ]);
    }

    /** @return array<string, PageRegistration> */
    public static function getPages(): array
    {
        return ['index' => ListFilestores::route('/'), 'create' => CreateFilestore::route('/create'), 'view' => ViewFilestore::route('/{record}'), 'edit' => EditFilestore::route('/{record}/edit')];
    }

    public static function environmentManaged(): bool
    {
        return app(FilestoreRegistry::class)->environmentManaged();
    }

    private static function fileAggregate(): \Illuminate\Database\Query\Builder
    {
        return DB::table('transfer_chunk_locations')
            ->join('transfer_chunks', 'transfer_chunks.id', '=', 'transfer_chunk_locations.transfer_chunk_id')
            ->join('transfer_items', 'transfer_items.id', '=', 'transfer_chunks.transfer_item_id')
            ->join('transfers', 'transfers.id', '=', 'transfer_items.transfer_id')
            ->selectRaw('count(distinct transfer_items.id)')
            ->where('transfers.kind', 'files')
            ->whereColumn('transfer_chunk_locations.filestore_id', 'filestores.id');
    }

    private static function noteAggregate(): \Illuminate\Database\Query\Builder
    {
        return DB::table('transfer_chunk_locations')
            ->join('transfer_chunks', 'transfer_chunks.id', '=', 'transfer_chunk_locations.transfer_chunk_id')
            ->join('transfer_items', 'transfer_items.id', '=', 'transfer_chunks.transfer_item_id')
            ->join('transfers', 'transfers.id', '=', 'transfer_items.transfer_id')
            ->selectRaw('count(distinct transfer_items.id)')
            ->where('transfers.kind', 'note')
            ->whereColumn('transfer_chunk_locations.filestore_id', 'filestores.id');
    }

    private static function driver(Filestore $filestore): string
    {
        return $filestore->driver ?? (string) config("filesystems.disks.{$filestore->disk_name}.driver", 'Laravel disk');
    }

    /** @return array<string> */
    private static function assignedPlanLabels(Filestore $filestore): array
    {
        return Plan::query()
            ->select('plans.name', 'plans.is_active', 'plan_filestore.is_default')
            ->join('plan_filestore', 'plan_filestore.plan_id', '=', 'plans.id')
            ->where('plan_filestore.filestore_id', $filestore->id)
            ->orderBy('plans.name')
            ->get()
            ->map(fn (Plan $plan): string => sprintf('%s%s%s', $plan->name, (bool) $plan->getAttribute('is_default') ? ' (default)' : '', $plan->is_active ? '' : ' (inactive)'))
            ->all();
    }
}
