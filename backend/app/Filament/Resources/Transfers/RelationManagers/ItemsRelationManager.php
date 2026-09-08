<?php

declare(strict_types=1);

namespace App\Filament\Resources\Transfers\RelationManagers;

use App\Filament\Resources\Transfers\TransferResource;
use App\Models\Transfer;
use App\Models\TransferItem;
use App\Models\User;
use Filament\Actions\Action;
use Filament\Facades\Filament;
use Filament\Infolists\Components\TextEntry;
use Filament\Resources\RelationManagers\RelationManager;
use Filament\Schemas\Components\Section;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Support\Number;

class ItemsRelationManager extends RelationManager
{
    protected static string $relationship = 'items';

    protected static ?string $title = 'Items';

    public function mount(): void
    {
        abort_unless(static::canViewForRecord($this->ownerRecord, $this->pageClass ?? static::class), 403);

        parent::mount();
    }

    public static function canViewForRecord(Model $ownerRecord, string $pageClass): bool
    {
        $actor = Filament::auth()->user();

        return $ownerRecord instanceof Transfer
            && $actor instanceof User
            && $actor->isStaff()
            && TransferResource::canView($ownerRecord);
    }

    public function table(Table $table): Table
    {
        return $table
            ->modifyQueryUsing(fn (Builder $query): Builder => $query->withCount('chunks'))
            ->columns([
                TextColumn::make('position')->label('Item')->prefix('#')->sortable(),
                TextColumn::make('receipt_status')->label('Status')->state(fn (TransferItem $record): string => $record->completed_at === null ? 'Pending' : 'Received'),
                TextColumn::make('received_chunks')->label('Chunks')->state(fn (TransferItem $record): string => $record->chunks_count.' / '.$record->chunk_count),
                TextColumn::make('ciphertext_bytes')->label('Received')->formatStateUsing(fn (int|string $state): string => Number::fileSize((int) $state, precision: 2)),
                TextColumn::make('declared_ciphertext_bytes')->label('Expected')->formatStateUsing(fn (int|string $state): string => Number::fileSize((int) $state, precision: 2)),
            ])
            ->recordActions([
                Action::make('diagnostics')
                    ->label('Diagnostics')
                    ->modal()
                    ->slideOver()
                    ->modalSubmitAction(false)
                    ->modalCancelActionLabel('Close')
                    ->schema([
                        Section::make('Chunk receipt')
                            ->schema([
                                TextEntry::make('position')->label('Item position')->prefix('#'),
                                TextEntry::make('chunk_count')->label('Expected chunks'),
                                TextEntry::make('received_chunks')->label('Received chunks')->state(fn (TransferItem $record): int => $record->chunks()->count()),
                                TextEntry::make('chunk_positions')->label('First 50 received positions')->state(function (TransferItem $record): string {
                                    $positions = $record->chunks()->orderBy('position')->limit(50)->pluck('position')->join(', ');

                                    return $positions === '' ? 'None' : $positions;
                                }),
                            ])
                            ->columns(2),
                    ])
                    ->action(fn (): null => null),
            ])
            ->defaultSort('position')
            ->paginated([10, 25, 50]);
    }
}
