<?php

declare(strict_types=1);

namespace App\Filament\Resources\FileReports;

use App\Actions\Admin\AssignFileReport;
use App\Actions\Admin\ReviewFileReport;
use App\Actions\Admin\TakeDownReportedTransfer;
use App\Enums\ReportStatus;
use App\Filament\Resources\FileReports\Pages\ListFileReports;
use App\Filament\Resources\FileReports\Pages\ViewFileReport;
use App\Filament\Resources\FileReports\RelationManagers\ActivityRelationManager;
use App\Filament\Resources\FileReports\RelationManagers\NotesRelationManager;
use App\Filament\Resources\FileReports\RelationManagers\RelatedReportsRelationManager;
use App\Filament\Resources\Transfers\TransferResource;
use App\Filament\Resources\Users\UserResource;
use App\Models\FileReport;
use App\Models\User;
use Filament\Actions\Action;
use Filament\Actions\ActionGroup;
use Filament\Facades\Filament;
use Filament\Forms\Components\Select;
use Filament\Forms\Components\Textarea;
use Filament\Infolists\Components\TextEntry;
use Filament\Resources\Pages\PageRegistration;
use Filament\Resources\RelationManagers\RelationManager;
use Filament\Resources\Resource;
use Filament\Schemas\Components\Section;
use Filament\Schemas\Schema;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Filters\SelectFilter;
use Filament\Tables\Table;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Support\Number;
use Illuminate\Support\Str;

class FileReportResource extends Resource
{
    protected static ?string $model = FileReport::class;

    protected static string|\UnitEnum|null $navigationGroup = 'Work';

    protected static ?int $navigationSort = 10;

    protected static string|\BackedEnum|null $navigationIcon = 'filebeam-reports';

    protected static ?string $modelLabel = 'File Report';

    protected static ?string $pluralModelLabel = 'File Reports';

    protected static ?string $recordTitleAttribute = 'id';

    public static function table(Table $table): Table
    {
        return $table
            ->columns([
                TextColumn::make('category')->label('Report')->formatStateUsing(fn (string $state): string => Str::headline($state))
                    ->description(fn (FileReport $record): string => Str::limit($record->description, 70)),
                TextColumn::make('id')->label('Case ID')->searchable()->copyable()->toggleable(isToggledHiddenByDefault: true),
                TextColumn::make('transfer_identifier')->label('Upload ID')->searchable()->copyable()->toggleable(isToggledHiddenByDefault: true),
                TextColumn::make('status')->badge(),
                TextColumn::make('assignee.email')->label('Assignee')->placeholder('Unassigned'),
                TextColumn::make('created_at')->label('Reported')->since()->tooltip(fn (FileReport $record): string => $record->created_at->toDayDateTimeString()),
            ])
            ->filters([SelectFilter::make('status')->options(self::statusOptions())])
            ->defaultSort('created_at')
            ->emptyStateHeading('No reports in this queue')
            ->emptyStateDescription('Try another queue or review reports as they arrive.')
            ->persistFiltersInSession()
            ->persistSearchInSession()
            ->recordUrl(fn (FileReport $record, ListFileReports $livewire): string => static::caseUrl($record, $livewire->activeTab))
            ->recordActions([
                Action::make('view')->label('Open case')->url(fn (FileReport $record, ListFileReports $livewire): string => static::caseUrl($record, $livewire->activeTab)),
                self::assignToMeAction(),
                ActionGroup::make([
                    self::assignAction(),
                    self::unassignAction(),
                    self::startReviewAction(),
                    self::resolveAction(),
                    self::dismissAction(),
                    self::reopenAction(),
                    self::takeDownAction(),
                ])->label('More')->button()->color('gray'),
            ]);
    }

    public static function infolist(Schema $schema): Schema
    {
        return $schema->components([
            Section::make('Report')->schema([
                TextEntry::make('id')->label('Case ID')->copyable(),
                TextEntry::make('status')->badge(),
                TextEntry::make('category')->formatStateUsing(fn (string $state): string => Str::headline($state)),
                TextEntry::make('reporter_email')->label('Reporter')->placeholder('Anonymous'),
                TextEntry::make('assignee.email')->label('Assignee')->placeholder('Unassigned'),
                TextEntry::make('created_at')->label('Reported')->dateTime(),
                TextEntry::make('description')->columnSpanFull()->wrap(),
                TextEntry::make('resolution')->columnSpanFull()->wrap()->placeholder('No decision recorded'),
            ])->columns(['default' => 1, 'sm' => 2])->columnSpan(['default' => 1, 'lg' => 2]),
            Section::make('Upload context')->schema([
                TextEntry::make('transfer_identifier')->label('Upload ID')->copyable(),
                TextEntry::make('transfer.status')->label('Lifecycle')->badge()->placeholder('Deleted'),
                TextEntry::make('transfer.kind')->label('Type')->badge()->placeholder('Unavailable'),
                TextEntry::make('transfer.item_count')->label('Items')->numeric()->placeholder('Unavailable'),
                TextEntry::make('transfer.ciphertext_bytes')->label('Uploaded size')->formatStateUsing(fn (?int $state): ?string => $state === null ? null : Number::fileSize($state, precision: 2))->placeholder('Unavailable'),
                TextEntry::make('transfer.expires_at')->label('Expires')->dateTime()->placeholder('Unavailable'),
                TextEntry::make('transfer_link')->label('Upload record')->state(fn (FileReport $record): ?string => $record->transfer === null ? null : 'Open upload')->url(fn (FileReport $record): ?string => $record->transfer === null ? null : TransferResource::getUrl('view', ['record' => $record->transfer]))->placeholder('Upload deleted'),
                TextEntry::make('owner_link')->label('Owner')->visible(fn (FileReport $record): bool => $record->transfer?->owner !== null && UserResource::canView($record->transfer->owner))->state(fn (): string => 'Open owner profile')->url(fn (FileReport $record): ?string => $record->transfer?->owner === null ? null : UserResource::getUrl('view', ['record' => $record->transfer->owner])),
                TextEntry::make('related_reports_count')->label('Other reports')->state(fn (FileReport $record): int => FileReport::query()->where('transfer_identifier', $record->transfer_identifier)->whereKeyNot($record->id)->count())->badge()->color('warning')->placeholder('No other reports'),
            ])->description('Related reports are not automatically closed when this case is decided.')->columns(1)->columnSpan(['default' => 1, 'lg' => 1]),
        ])->columns(['default' => 1, 'lg' => 3]);
    }

    /** @return array<string, PageRegistration> */
    public static function getPages(): array
    {
        return [
            'index' => ListFileReports::route('/'),
            'view' => ViewFileReport::route('/{record}'),
        ];
    }

    /** @return array<class-string<RelationManager>> */
    public static function getRelations(): array
    {
        return [NotesRelationManager::class, ActivityRelationManager::class, RelatedReportsRelationManager::class];
    }

    public static function getEloquentQuery(): Builder
    {
        return parent::getEloquentQuery()->with(['assignee', 'transfer.owner']);
    }

    /** @return array<string> */
    public static function getGloballySearchableAttributes(): array
    {
        return ['id', 'transfer_identifier'];
    }

    public static function caseUrl(FileReport $report, ?string $queueTab = null): string
    {
        $parameters = ['record' => $report];

        if (in_array($queueTab, ['mine', 'unassigned', 'active', 'closed'], true)) {
            $parameters['queue'] = $queueTab;
        }

        return static::getUrl('view', $parameters);
    }

    public static function assignAction(): Action
    {
        return Action::make('assign')
            ->label('Reassign or unassign')
            ->modalSubmitActionLabel('Save assignment')
            ->authorize('assign')
            ->schema([self::assigneeField()->nullable()])
            ->successNotificationTitle('Assignment updated')
            ->fillForm(fn (FileReport $record): array => ['assigned_to' => $record->assigned_to])
            ->action(fn (array $data, FileReport $record, AssignFileReport $assign) => $assign->handle(self::actor(), $record, filled($data['assigned_to']) ? (int) $data['assigned_to'] : null));
    }

    public static function assignToMeAction(): Action
    {
        return Action::make('assignToMe')
            ->label('Assign to me')
            ->authorize('assign')
            ->visible(fn (FileReport $record): bool => $record->assigned_to !== self::actor()->id)
            ->action(fn (FileReport $record, AssignFileReport $assign) => $assign->handle(self::actor(), $record, self::actor()->id))
            ->successNotificationTitle('Case assigned to you');
    }

    public static function unassignAction(): Action
    {
        return Action::make('unassign')
            ->label('Unassign')
            ->authorize('assign')
            ->visible(fn (FileReport $record): bool => $record->assigned_to !== null)
            ->requiresConfirmation()
            ->successNotificationTitle('Case returned to the unassigned queue')
            ->action(fn (FileReport $record, AssignFileReport $assign) => $assign->handle(self::actor(), $record, null));
    }

    public static function startReviewAction(): Action
    {
        return Action::make('startReview')
            ->label('Start review')
            ->authorize('review')
            ->visible(fn (FileReport $record): bool => $record->status === ReportStatus::Open)
            ->successNotificationTitle('Review started')
            ->action(fn (FileReport $record, ReviewFileReport $review) => $review->handle(self::actor(), $record, ReportStatus::InReview, null));
    }

    public static function resolveAction(): Action
    {
        return self::decisionAction('resolve', 'Resolve without takedown', ReportStatus::Resolved, 'success');
    }

    public static function dismissAction(): Action
    {
        return self::decisionAction('dismiss', 'Dismiss', ReportStatus::Dismissed, 'gray');
    }

    public static function reopenAction(): Action
    {
        return Action::make('reopen')
            ->label('Reopen')
            ->modalSubmitActionLabel('Reopen report')
            ->authorize('review')
            ->visible(fn (FileReport $record): bool => in_array($record->status, [ReportStatus::Resolved, ReportStatus::Dismissed], true))
            ->modalDescription('Reopening preserves the prior decision in the activity history. Explain why further review is needed.')
            ->schema([Textarea::make('reason')->required()->maxLength(5000)])
            ->successNotificationTitle('Case reopened')
            ->action(fn (array $data, FileReport $record, ReviewFileReport $review) => $review->reopen(self::actor(), $record, $data['reason']));
    }

    public static function takeDownAction(): Action
    {
        return Action::make('takedown')
            ->label('Take down upload')
            ->modalSubmitActionLabel('Make unavailable and resolve')
            ->color('danger')
            ->authorize('takedown')
            ->visible(fn (FileReport $record): bool => $record->transfer_id !== null && ! in_array($record->status, [ReportStatus::Resolved, ReportStatus::Dismissed], true))
            ->modalDescription('This queues upload deletion and resolves this case. Other reports for the upload remain open.')
            ->schema([Textarea::make('reason')->required()->maxLength(2000)])
            ->action(fn (array $data, FileReport $record, TakeDownReportedTransfer $takedown) => $takedown->handle(self::actor(), $record, $data['reason']))
            ->successNotificationTitle('Upload deletion queued');
    }

    private static function decisionAction(string $name, string $label, ReportStatus $status, string $color): Action
    {
        return Action::make($name)
            ->label($label)
            ->modalSubmitActionLabel($status === ReportStatus::Resolved ? 'Resolve report' : 'Dismiss report')
            ->color($color)
            ->authorize('review')
            ->visible(fn (FileReport $record): bool => in_array($record->status, [ReportStatus::Open, ReportStatus::InReview], true))
            ->modalDescription('Record the decision for this case. This does not alter other reports for the upload.')
            ->schema([Textarea::make('resolution')->required()->maxLength(5000)])
            ->successNotificationTitle($status === ReportStatus::Resolved ? 'Case resolved' : 'Case dismissed')
            ->action(fn (array $data, FileReport $record, ReviewFileReport $review) => $review->handle(self::actor(), $record, $status, $data['resolution']));
    }

    private static function assigneeField(): Select
    {
        return Select::make('assigned_to')->label('Assignee')->options(fn (): array => User::query()
            ->activeStaff()->orderBy('email')->pluck('email', 'id')->all())->searchable();
    }

    /** @return array<string, string> */
    private static function statusOptions(): array
    {
        return collect(ReportStatus::cases())->mapWithKeys(fn (ReportStatus $status): array => [$status->value => $status->getLabel()])->all();
    }

    private static function actor(): User
    {
        $actor = Filament::auth()->user();
        abort_unless($actor instanceof User, 403);

        return $actor;
    }
}
