<?php

declare(strict_types=1);

namespace App\Filament\Resources\Transfers\RelationManagers;

use App\Filament\Resources\FileReports\FileReportResource;
use App\Models\FileReport;
use Filament\Actions\Action;
use Filament\Resources\RelationManagers\RelationManager;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;

class ReportsRelationManager extends RelationManager
{
    protected static string $relationship = 'reports';

    protected static ?string $relatedResource = FileReportResource::class;

    protected static ?string $title = 'Reports';

    public function table(Table $table): Table
    {
        return $table
            ->columns([
                TextColumn::make('id')->label('ID')->copyable(),
                TextColumn::make('category'),
                TextColumn::make('status')->badge(),
                TextColumn::make('created_at')->label('Reported')->dateTime(),
            ])
            ->recordActions([
                Action::make('open')
                    ->label('Open report')
                    ->url(fn (FileReport $record): ?string => FileReportResource::hasPage('view') && FileReportResource::canView($record) ? FileReportResource::getUrl('view', ['record' => $record]) : null),
            ])
            ->defaultSort('created_at', 'desc');
    }
}
