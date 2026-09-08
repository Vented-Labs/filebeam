<?php

declare(strict_types=1);

namespace App\Filament\Widgets;

use App\Filament\Resources\FileReports\FileReportResource;
use App\Models\FileReport;
use App\Models\User;
use Filament\Actions\Action;
use Filament\Facades\Filament;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;
use Filament\Widgets\TableWidget;
use Illuminate\Support\Str;

class ActiveReports extends TableWidget
{
    protected static ?int $sort = 2;

    protected int|string|array $columnSpan = 'full';

    public static function canView(): bool
    {
        $actor = Filament::auth()->user();

        return $actor instanceof User && $actor->isStaff();
    }

    public function table(Table $table): Table
    {
        return $table
            ->heading('Oldest active reports')
            ->description('Start with the reports that have been waiting longest.')
            ->query(FileReport::query()->active()->with('assignee'))
            ->defaultSort('created_at')
            ->columns([
                TextColumn::make('category')->label('Report')->formatStateUsing(fn (string $state): string => Str::headline($state))
                    ->description(fn (FileReport $record): string => Str::limit($record->description, 70)),
                TextColumn::make('status')->badge(),
                TextColumn::make('assignee.email')->label('Assignee')->placeholder('Unassigned'),
                TextColumn::make('created_at')->label('Waiting since')->since()->dateTimeTooltip()->sortable(),
            ])
            ->recordUrl(fn (FileReport $record): string => FileReportResource::getUrl('view', ['record' => $record, 'queue' => 'active']))
            ->headerActions([
                Action::make('myQueue')->label('My queue')->url(FileReportResource::getUrl('index', ['tab' => 'mine'])),
                Action::make('allReports')->label('All active reports')->url(FileReportResource::getUrl('index', ['tab' => 'active'])),
            ])
            ->paginated([5, 10, 25])
            ->defaultPaginationPageOption(5)
            ->emptyStateHeading('No reports need review')
            ->emptyStateDescription('New reports will appear here. Closed cases are available in the report history.');
    }
}
