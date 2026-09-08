<?php

declare(strict_types=1);

use App\Enums\TransferStatus;
use App\Enums\UserRole;
use App\Filament\Resources\Transfers\Pages\ListTransfers;
use App\Filament\Resources\Transfers\Pages\ViewTransfer;
use App\Filament\Resources\Transfers\RelationManagers\ActivityRelationManager;
use App\Filament\Resources\Transfers\RelationManagers\ItemsRelationManager;
use App\Filament\Resources\Transfers\TransferResource;
use App\Models\AdminAudit;
use App\Models\Transfer;
use App\Models\TransferChunk;
use App\Models\TransferItem;
use App\Models\User;
use Filament\Facades\Filament;
use Filament\Notifications\Notification;
use Illuminate\Support\Facades\Queue;
use Livewire\Livewire;

test('staff can open the safe upload workspace and its item diagnostics', function () {
    $staff = User::factory()->create(['role' => UserRole::Moderator]);
    $transfer = Transfer::factory()->for($staff, 'owner')->create([
        'status' => TransferStatus::Available,
        'encrypted_manifest' => 'never-send-this-manifest',
        'upload_token_hash' => 'never-send-upload-token',
        'delete_token_hash' => 'never-send-delete-token',
        'read_token_hash' => 'never-send-read-token',
        'declared_ciphertext_bytes' => 2 * 1024 * 1024,
        'ciphertext_bytes' => 1024 * 1024,
    ]);
    $item = TransferItem::factory()->for($transfer)->create(['position' => 1, 'chunk_count' => 3]);
    TransferChunk::factory()->for($item, 'item')->create(['position' => 0]);
    TransferChunk::factory()->for($item, 'item')->create(['position' => 1]);

    $this->actingAs($staff, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListTransfers::class)
        ->searchTable($transfer->id)
        ->assertCanSeeTableRecords([$transfer]);

    expect(TransferResource::getGloballySearchableAttributes())->toBe(['id']);

    Livewire::test(ViewTransfer::class, ['record' => $transfer->id])
        ->assertSee('Upload progress')
        ->assertSee('Expected')
        ->assertSee('Encryption boundary')
        ->assertDontSee('never-send-this-manifest')
        ->assertDontSee('never-send-upload-token')
        ->assertDontSee('never-send-delete-token')
        ->assertDontSee('never-send-read-token');

    Livewire::test(ItemsRelationManager::class, [
        'ownerRecord' => $transfer,
        'pageClass' => ViewTransfer::class,
    ])
        ->assertCanSeeTableRecords([$item])
        ->assertTableActionExists('diagnostics', null, $item);
});

test('takedown confirms queued cleanup and moderator workspace links stay unprivileged', function () {
    Queue::fake();
    $moderator = User::factory()->create(['role' => UserRole::Moderator]);
    $owner = User::factory()->create();
    $transfer = Transfer::factory()->for($owner, 'owner')->create(['status' => TransferStatus::Available]);

    $this->actingAs($moderator, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ViewTransfer::class, ['record' => $transfer->id])
        ->assertDontSee('/admin/users/'.$owner->id)
        ->callAction('takedown', ['reason' => 'Confirmed abuse'])
        ->assertHasNoActionErrors();

    Notification::assertNotified('Upload unavailable; cleanup queued.');
    expect($transfer->refresh()->status)->toBe(TransferStatus::Deleting);
});

test('transfer relation managers require staff access to their parent upload', function () {
    $moderator = User::factory()->create(['role' => UserRole::Moderator]);
    $transfer = Transfer::factory()->create();
    $audit = AdminAudit::factory()->create([
        'target_type' => Transfer::class,
        'target_id' => $transfer->id,
        'action' => 'transfer.takedown_requested',
    ]);

    $this->actingAs($moderator, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ActivityRelationManager::class, [
        'ownerRecord' => $transfer,
        'pageClass' => ViewTransfer::class,
    ])->assertCanSeeTableRecords([$audit]);

    $this->actingAs(User::factory()->create(), 'admin');

    Livewire::test(ActivityRelationManager::class, [
        'ownerRecord' => $transfer,
        'pageClass' => ViewTransfer::class,
    ])->assertForbidden();

    Livewire::test(ItemsRelationManager::class, [
        'ownerRecord' => $transfer,
        'pageClass' => ViewTransfer::class,
    ])->assertForbidden();
});
