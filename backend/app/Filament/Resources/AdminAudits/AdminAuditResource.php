<?php

declare(strict_types=1);

namespace App\Filament\Resources\AdminAudits;

use App\Filament\Resources\AdminAudits\Pages\ListAdminAudits;
use App\Filament\Resources\AdminAudits\Pages\ViewAdminAudit;
use App\Filament\Resources\FileReports\FileReportResource;
use App\Filament\Resources\Plans\PlanResource;
use App\Filament\Resources\Transfers\TransferResource;
use App\Filament\Resources\Users\UserResource;
use App\Filament\Support\AuditPresentation;
use App\Models\AdminAudit;
use App\Models\FileReport;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\User;
use Carbon\CarbonImmutable;
use Filament\Actions\Action;
use Filament\Forms\Components\DatePicker;
use Filament\Infolists\Components\RepeatableEntry;
use Filament\Infolists\Components\TextEntry;
use Filament\Resources\Pages\PageRegistration;
use Filament\Resources\Resource;
use Filament\Schemas\Components\Section;
use Filament\Schemas\Schema;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Filters\Filter;
use Filament\Tables\Filters\SelectFilter;
use Filament\Tables\Table;
use Illuminate\Database\Eloquent\Builder;

class AdminAuditResource extends Resource
{
    protected static ?string $model = AdminAudit::class;

    protected static ?string $modelLabel = 'Audit Entry';

    protected static ?string $pluralModelLabel = 'Audit History';

    protected static ?string $recordTitleAttribute = 'action';

    protected static string|\UnitEnum|null $navigationGroup = 'Administration';

    protected static string|\BackedEnum|null $navigationIcon = 'filebeam-clock';

    protected static ?int $navigationSort = 50;

    protected static bool $isGloballySearchable = false;

    public static function table(Table $table): Table
    {
        return $table
            ->defaultSort('created_at', 'desc')
            ->columns([
                TextColumn::make('created_at')->label('When')->dateTime()->sortable(),
                TextColumn::make('actor.email')->label('Actor')->placeholder('System / former staff'),
                TextColumn::make('action')->label('Event')->formatStateUsing(fn (string $state): string => AuditPresentation::label($state))->searchable(),
                TextColumn::make('target_type')->label('Record')->formatStateUsing(fn (string $state): string => class_basename($state)),
                TextColumn::make('target_id')->label('Target ID')->searchable(),
                TextColumn::make('reason')->limit(80)->placeholder('-'),
            ])
            ->filters([
                SelectFilter::make('actor')->relationship('actor', 'email')->searchable(),
                SelectFilter::make('target_type')->label('Record type')->options([
                    User::class => 'Accounts', Plan::class => 'Plans', Transfer::class => 'Uploads', FileReport::class => 'Reports',
                ]),
                Filter::make('date_range')->schema([DatePicker::make('from'), DatePicker::make('until')])
                    ->query(fn (Builder $query, array $data): Builder => $query
                        ->when($data['from'] ?? null, fn (Builder $query, string $date): Builder => $query->where('created_at', '>=', CarbonImmutable::parse($date, config('app.timezone'))->startOfDay()))
                        ->when($data['until'] ?? null, fn (Builder $query, string $date): Builder => $query->where('created_at', '<', CarbonImmutable::parse($date, config('app.timezone'))->startOfDay()->addDay()))),
            ])
            ->recordActions([
                Action::make('view')
                    ->label('Quick view')
                    ->modal()
                    ->authorize('view')
                    ->slideOver()
                    ->modalHeading('Audit entry')
                    ->modalSubmitAction(false)
                    ->modalCancelActionLabel('Close')
                    ->schema(self::details())
                    ->action(fn (): null => null),
            ])
            ->recordUrl(fn (AdminAudit $record): string => self::getUrl('view', ['record' => $record]))
            ->emptyStateHeading('No audit events match')
            ->emptyStateDescription('Try another actor, record type, or date range.');
    }

    /** @return array<string, PageRegistration> */
    public static function getPages(): array
    {
        return ['index' => ListAdminAudits::route('/'), 'view' => ViewAdminAudit::route('/{record}')];
    }

    public static function infolist(Schema $schema): Schema
    {
        return $schema->components(self::details());
    }

    public static function getEloquentQuery(): Builder
    {
        return parent::getEloquentQuery()->with('actor');
    }

    /** @return array<Section> */
    private static function details(): array
    {
        return [
            Section::make('Event')->columnSpanFull()->columns(['default' => 1, 'md' => 2])->schema([
                TextEntry::make('action')->label('What happened')->formatStateUsing(fn (string $state): string => AuditPresentation::label($state)),
                TextEntry::make('created_at')->label('When')->dateTime(),
                TextEntry::make('actor.email')->label('Actor')->placeholder('System / former staff'),
                TextEntry::make('target_id')->label('Record')->copyable()->url(fn (AdminAudit $record): ?string => self::targetUrl($record)),
                TextEntry::make('reason')->placeholder('No reason recorded')->wrap()->columnSpanFull(),
            ]),
            Section::make('Recorded changes')->columnSpanFull()->description('Only supported account, plan, and moderation fields are shown.')->schema([
                RepeatableEntry::make('safe_changes')->hiddenLabel()->state(fn (AdminAudit $record): array => AuditPresentation::changes($record))
                    ->schema([
                        TextEntry::make('field')->label('Field'),
                        TextEntry::make('before')->label('Before')->wrap(),
                        TextEntry::make('after')->label('After')->wrap(),
                    ])->columns(['default' => 1, 'md' => 3]),
            ]),
        ];
    }

    private static function targetUrl(AdminAudit $audit): ?string
    {
        $resource = match ($audit->target_type) {
            User::class => UserResource::class,
            Plan::class => PlanResource::class,
            FileReport::class => FileReportResource::class,
            Transfer::class => TransferResource::class,
            default => null,
        };

        if ($resource === null || (in_array($audit->target_type, [User::class, Plan::class], true) && ! ctype_digit($audit->target_id))) {
            return null;
        }

        $record = $resource::getEloquentQuery()->find($audit->target_id);

        return $record !== null && $resource::canView($record) ? $resource::getUrl('view', ['record' => $record]) : null;
    }
}
