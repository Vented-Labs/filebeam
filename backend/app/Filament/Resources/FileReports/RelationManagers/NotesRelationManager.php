<?php

declare(strict_types=1);

namespace App\Filament\Resources\FileReports\RelationManagers;

use App\Actions\Admin\AddReportNote;
use App\Models\FileReport;
use App\Models\User;
use Filament\Actions\Action;
use Filament\Facades\Filament;
use Filament\Forms\Components\Textarea;
use Filament\Resources\RelationManagers\RelationManager;
use Filament\Tables\Columns\Layout\Split;
use Filament\Tables\Columns\Layout\Stack;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Support\Facades\Gate;

class NotesRelationManager extends RelationManager
{
    protected static string $relationship = 'notes';

    protected static ?string $title = 'Staff notes';

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
            ->columns([
                Stack::make([
                    TextColumn::make('body')->label('Note')->wrap(),
                    Split::make([
                        TextColumn::make('author.email')->label('Author')->placeholder('Former staff member'),
                        TextColumn::make('created_at')->label('Added')->since()->tooltip(fn ($record): string => $record->created_at->toDayDateTimeString())->alignEnd(),
                    ])->from('sm'),
                ])->space(2),
            ])
            ->defaultSort('created_at', 'desc')
            ->emptyStateHeading('No staff notes yet')
            ->emptyStateDescription('Add an internal plain-text note to record context for the next reviewer. Notes cannot be edited or deleted.')
            ->headerActions([
                Action::make('addNote')
                    ->label('Add note')
                    ->modalSubmitActionLabel('Save note')
                    ->successNotificationTitle('Internal note added')
                    ->authorize('create')
                    ->modalDescription('Internal plain text only. Do not include upload keys, passwords, or other secrets. Notes cannot be edited or deleted.')
                    ->schema([Textarea::make('body')->label('Internal note')->rows(5)->required()->maxLength(5000)])
                    ->action(function (array $data, AddReportNote $notes): void {
                        $actor = Filament::auth()->user();
                        abort_unless($actor instanceof User, 403);
                        $owner = $this->getOwnerRecord();
                        abort_unless($owner instanceof FileReport, 404);

                        $notes->handle($actor, $owner, $data['body']);
                    }),
            ]);
    }
}
