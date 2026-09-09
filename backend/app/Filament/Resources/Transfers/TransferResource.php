<?php

declare(strict_types=1);

namespace App\Filament\Resources\Transfers;

use App\Actions\Admin\RequestTransferTakedown;
use App\Actions\Admin\RetryTransferCleanup;
use App\Enums\ReportStatus;
use App\Enums\TransferKind;
use App\Enums\TransferStatus;
use App\Filament\Resources\Plans\PlanResource;
use App\Filament\Resources\Transfers\Pages\ListTransfers;
use App\Filament\Resources\Transfers\Pages\ViewTransfer;
use App\Filament\Resources\Transfers\RelationManagers\ActivityRelationManager;
use App\Filament\Resources\Transfers\RelationManagers\ItemsRelationManager;
use App\Filament\Resources\Transfers\RelationManagers\ReportsRelationManager;
use App\Filament\Resources\Users\UserResource;
use App\Models\FileReport;
use App\Models\Transfer;
use App\Models\User;
use Filament\Actions\Action;
use Filament\Facades\Filament;
use Filament\Forms\Components\DateTimePicker;
use Filament\Forms\Components\Textarea;
use Filament\Infolists\Components\TextEntry;
use Filament\Notifications\Notification;
use Filament\Resources\Pages\PageRegistration;
use Filament\Resources\RelationManagers\RelationManager;
use Filament\Resources\Resource;
use Filament\Schemas\Components\Section;
use Filament\Schemas\Schema;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Filters\Filter;
use Filament\Tables\Filters\SelectFilter;
use Filament\Tables\Table;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Support\Number;

class TransferResource extends Resource
{
    protected static ?string $model = Transfer::class;

    protected static ?string $modelLabel = 'Upload';

    protected static ?string $pluralModelLabel = 'Uploads';

    protected static ?string $recordTitleAttribute = 'id';

    protected static ?string $slug = 'transfers';

    protected static string|\BackedEnum|null $navigationIcon = 'filebeam-transfers';

    protected static string|\UnitEnum|null $navigationGroup = 'Work';

    protected static ?int $navigationSort = 20;

    public static function table(Table $table): Table
    {
        return $table
            ->defaultSort('created_at', 'desc')
            ->columns([
                TextColumn::make('id')->label('ID')->searchable()->copyable(),
                TextColumn::make('owner.email')->label('Owner')->searchable()->placeholder('Anonymous'),
                TextColumn::make('kind')->badge(),
                TextColumn::make('status')->badge(),
                self::fileSizeColumn('declared_ciphertext_bytes', 'Expected size'),
                self::fileSizeColumn('ciphertext_bytes', 'Received size')->placeholder('-'),
                TextColumn::make('item_count')->label('Items')->numeric(),
                TextColumn::make('reports_count')->label('Reports')->numeric(),
                TextColumn::make('expires_at')->label('Expires')->since()->tooltip(fn (Transfer $record): string => $record->expires_at->toDayDateTimeString())->sortable(),
            ])
            ->filters([
                SelectFilter::make('kind')->options(self::enumOptions(TransferKind::cases())),
                SelectFilter::make('status')->options(self::enumOptions(TransferStatus::cases())),
                Filter::make('expires_between')
                    ->schema([
                        DateTimePicker::make('from')->label('Expires after'),
                        DateTimePicker::make('until')->label('Expires before'),
                    ])
                    ->query(function (Builder $query, array $data): Builder {
                        return $query
                            ->when($data['from'] ?? null, fn (Builder $query, string $date): Builder => $query->where('expires_at', '>=', $date))
                            ->when($data['until'] ?? null, fn (Builder $query, string $date): Builder => $query->where('expires_at', '<=', $date));
                    }),
                Filter::make('has_open_reports')
                    ->label('Open reports')
                    ->query(fn (Builder $query): Builder => $query->whereExists(
                        FileReport::query()
                            ->selectRaw('1')
                            ->whereColumn('transfer_id', 'transfers.id')
                            ->where('status', ReportStatus::Open),
                    )),
            ])
            ->recordActions([
                self::quickViewAction(),
                self::takedownAction(),
                self::retryCleanupAction(),
            ])
            ->recordUrl(fn (Transfer $record): string => self::getUrl('view', ['record' => $record]))
            ->emptyStateHeading('No uploads match this workspace')
            ->emptyStateDescription('Try another lifecycle tab or clear a filter to review uploads.');
    }

    /** @return array<string> */
    public static function getGloballySearchableAttributes(): array
    {
        return ['id'];
    }

    /** @return array<string, PageRegistration> */
    public static function getPages(): array
    {
        return [
            'index' => ListTransfers::route('/'),
            'view' => ViewTransfer::route('/{record}'),
        ];
    }

    /** @return array<class-string<RelationManager>> */
    public static function getRelations(): array
    {
        return [
            ItemsRelationManager::class,
            ReportsRelationManager::class,
            ActivityRelationManager::class,
        ];
    }

    public static function getEloquentQuery(): Builder
    {
        return parent::getEloquentQuery()
            ->with(['owner', 'plan'])
            ->select('transfers.*')
            ->selectSub(
                FileReport::query()
                    ->selectRaw('count(*)')
                    ->whereColumn('transfer_id', 'transfers.id'),
                'reports_count',
            );
    }

    public static function infolist(Schema $schema): Schema
    {
        return $schema->components(self::workspaceSchema());
    }

    /** @return array<Section> */
    private static function workspaceSchema(): array
    {
        return [
            Section::make('Lifecycle')
                ->schema([
                    TextEntry::make('id')->label('ID')->copyable(),
                    TextEntry::make('kind')->badge(),
                    TextEntry::make('delivery')->badge(),
                    TextEntry::make('status')->badge(),
                    TextEntry::make('completed_at')->label('Completed')->dateTime()->placeholder('Not completed'),
                    TextEntry::make('expires_at')->label('Expires')->since()->tooltip(fn (Transfer $record): string => $record->expires_at->toDayDateTimeString()),
                ])
                ->columns(['default' => 1, 'md' => 2, 'xl' => 3])
                ->columnSpanFull(),
            Section::make('Upload progress')
                ->schema([
                    self::fileSizeEntry('declared_ciphertext_bytes', 'Expected'),
                    self::fileSizeEntry('ciphertext_bytes', 'Received'),
                    TextEntry::make('expected_chunks')->label('Expected chunks')->state(fn (Transfer $record): int => (int) $record->items()->sum('chunk_count')),
                    TextEntry::make('received_chunks')->label('Received chunks')->state(fn (Transfer $record): int => (int) $record->items()->withCount('chunks')->get()->sum('chunks_count')),
                ])
                ->columns(['default' => 1, 'md' => 2, 'xl' => 4])
                ->columnSpanFull(),
            Section::make('Delivery and retention')
                ->schema([
                    TextEntry::make('delivery')->badge(),
                    TextEntry::make('retention_hours')->label('Retention')->suffix(' hours'),
                    TextEntry::make('burn_on_read')->label('Burn on read')->formatStateUsing(fn (bool $state): string => $state ? 'Enabled' : 'Disabled'),
                ])
                ->columns(['default' => 1, 'md' => 3])
                ->columnSpanFull(),
            Section::make('Account context')
                ->schema([
                    TextEntry::make('owner.email')
                        ->label('Owner')
                        ->placeholder('Anonymous')
                        ->url(fn (Transfer $record): ?string => $record->owner !== null && UserResource::canView($record->owner) ? UserResource::getUrl('view', ['record' => $record->owner]) : null),
                    TextEntry::make('plan.name')
                        ->label('Plan')
                        ->placeholder('Unavailable')
                        ->url(fn (Transfer $record): ?string => $record->plan !== null && PlanResource::canView($record->plan) ? PlanResource::getUrl('view', ['record' => $record->plan]) : null),
                ])
                ->columns(['default' => 1, 'md' => 2])
                ->columnSpanFull(),
            Section::make('Encryption boundary')
                ->description('Filenames, manifest contents, storage paths, keys, and access tokens are encrypted or secret and are intentionally not shown to staff.')
                ->columnSpanFull(),
        ];
    }

    public static function quickViewAction(): Action
    {
        return Action::make('view')
            ->label('Quick view')
            ->modal()
            ->authorize('view')
            ->slideOver()
            ->modalHeading('Upload overview')
            ->modalSubmitAction(false)
            ->modalCancelActionLabel('Close')
            ->schema(self::workspaceSchema())
            ->action(fn (): null => null);
    }

    public static function takedownAction(): Action
    {
        return Action::make('takedown')
            ->label('Make unavailable')
            ->modalSubmitActionLabel('Make unavailable')
            ->color('danger')
            ->visible(fn (Transfer $record): bool => $record->status !== TransferStatus::Deleting)
            ->authorize('takedown')
            ->modalDescription('This makes the upload unavailable and queues storage cleanup. It is not deleted immediately.')
            ->schema([self::reasonField()])
            ->action(function (array $data, Transfer $record, RequestTransferTakedown $takedown): void {
                $takedown->handle(self::actor(), $record, $data['reason']);

                Notification::make()->title('Upload unavailable; cleanup queued.')->success()->send();
            });
    }

    public static function retryCleanupAction(): Action
    {
        return Action::make('retryCleanup')
            ->label('Retry cleanup')
            ->modalSubmitActionLabel('Queue cleanup retry')
            ->color('warning')
            ->visible(fn (Transfer $record): bool => $record->status === TransferStatus::Deleting)
            ->authorize('retryCleanup')
            ->modalDescription('This queues another cleanup attempt. The upload remains unavailable.')
            ->schema([self::reasonField()])
            ->action(function (array $data, Transfer $record, RetryTransferCleanup $cleanup): void {
                $cleanup->handle(self::actor(), $record, $data['reason']);

                Notification::make()->title('Cleanup retry queued.')->success()->send();
            });
    }

    private static function reasonField(): Textarea
    {
        return Textarea::make('reason')->required()->maxLength(2000);
    }

    private static function fileSizeColumn(string $name, string $label): TextColumn
    {
        return TextColumn::make($name)
            ->label($label)
            ->formatStateUsing(fn (int|string|null $state): string => self::fileSize($state))
            ->tooltip(fn (Transfer $record) => number_format((int) $record->getAttribute($name)).' bytes');
    }

    private static function fileSizeEntry(string $name, string $label): TextEntry
    {
        return TextEntry::make($name)
            ->label($label)
            ->formatStateUsing(fn (int|string|null $state): string => self::fileSize($state))
            ->tooltip(fn (Transfer $record) => number_format((int) $record->getAttribute($name)).' bytes');
    }

    private static function fileSize(int|string|null $bytes): string
    {
        return Number::fileSize((int) ($bytes ?? 0), precision: 2);
    }

    /**
     * @param  array<TransferKind|TransferStatus>  $cases
     * @return array<string, string>
     */
    private static function enumOptions(array $cases): array
    {
        return collect($cases)->mapWithKeys(fn (TransferKind|TransferStatus $case): array => [$case->value => $case->getLabel()])->all();
    }

    private static function actor(): User
    {
        $actor = Filament::auth()->user();

        abort_unless($actor instanceof User, 403);

        return $actor;
    }
}
