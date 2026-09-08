<?php

declare(strict_types=1);

namespace App\Filament\Resources\FileReports\RelationManagers;

use App\Filament\Resources\FileReports\FileReportResource;
use Filament\Resources\RelationManagers\RelationManager;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;

class RelatedReportsRelationManager extends RelationManager
{
    protected static string $relationship = 'relatedReports';

    protected static ?string $title = 'Related reports';

    public function table(Table $table): Table
    {
        return $table
            ->columns([
                TextColumn::make('id')->label('Case ID')->copyable(),
                TextColumn::make('category'),
                TextColumn::make('status')->badge(),
                TextColumn::make('created_at')->label('Reported')->since(),
            ])
            ->recordUrl(fn ($record): string => FileReportResource::getUrl('view', ['record' => $record]));
    }
}
