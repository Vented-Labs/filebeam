<?php

declare(strict_types=1);

namespace App\Filament\Resources\FileReports\RelationManagers;

use App\Filament\Support\AuditPresentation;
use App\Models\AdminAudit;
use App\Models\FileReport;
use App\Models\User;
use Filament\Facades\Filament;
use Filament\Resources\RelationManagers\RelationManager;
use Filament\Tables\Columns\Layout\Split;
use Filament\Tables\Columns\Layout\Stack;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Support\Facades\Gate;

class ActivityRelationManager extends RelationManager
{
    protected static string $relationship = 'activity';

    protected static ?string $title = 'Case activity';

    public function mount(): void
    {
        parent::mount();

        abort_unless(static::canViewForRecord($this->ownerRecord, $this->pageClass ?? static::class), 403);
    }

    public static function canViewForRecord(Model $ownerRecord, string $pageClass): bool
    {
        $actor = Filament::auth()->user();

        return $ownerRecord instanceof FileReport
            && $actor instanceof User
            && $actor->isStaff()
            && Gate::forUser($actor)->allows('view', $ownerRecord);
    }

    public function table(Table $table): Table
    {
        return $table
            ->modifyQueryUsing(fn (Builder $query): Builder => $query->whereIn('action', [
                'file_report.assigned',
                'file_report.reviewed',
                'file_report.reopened',
            ]))
            ->defaultSort(fn (Builder $query): Builder => $query->orderByDesc('created_at')->orderByDesc('id'))
            ->columns([
                Stack::make([
                    Split::make([
                        TextColumn::make('action')
                            ->label('Event')
                            ->formatStateUsing(fn (string $state): string => AuditPresentation::label($state))
                            ->description(fn (AdminAudit $record): string => $record->created_at->toDayDateTimeString()),
                        TextColumn::make('actor.email')
                            ->label('Staff member')
                            ->placeholder('Former staff member')
                            ->alignEnd(),
                    ])->from('sm'),
                    TextColumn::make('reason')->label('Reason')->wrap()->placeholder('No reason recorded'),
                    TextColumn::make('changes')
                        ->label('Changes')
                        ->state(fn (AdminAudit $record): array => collect(AuditPresentation::changes($record))
                            ->map(fn (array $change): string => sprintf('%s: %s -> %s', $change['field'], $change['before'], $change['after']))
                            ->all())
                        ->listWithLineBreaks()
                        ->wrap()
                        ->placeholder('No recorded field changes'),
                ])->space(2),
            ]);
    }
}
