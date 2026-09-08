<?php

declare(strict_types=1);

use App\Actions\Admin\ManageFilestore;
use App\Actions\Admin\ManagePlan;
use App\Enums\TransferKind;
use App\Enums\TransferStatus;
use App\Enums\UserRole;
use App\Filament\Resources\Filestores\FilestoreResource;
use App\Filament\Resources\Filestores\Pages\CreateFilestore;
use App\Filament\Resources\Filestores\Pages\EditFilestore;
use App\Filament\Resources\Filestores\Pages\ListFilestores;
use App\Filament\Resources\Filestores\Pages\ViewFilestore;
use App\Models\AdminAudit;
use App\Models\Filestore;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferChunk;
use App\Models\TransferChunkLocation;
use App\Models\TransferChunkUpload;
use App\Models\TransferItem;
use App\Models\User;
use App\Support\FilestoreRegistry;
use Filament\Facades\Filament;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Illuminate\Validation\ValidationException;
use Livewire\Livewire;

uses(RefreshDatabase::class);

function filestoreAdministrator(): User
{
    return User::factory()->create(['role' => UserRole::Admin]);
}

beforeEach(function (): void {
    config()->set('filebeam.filesystems.local_root', storage_path('app'));
    config()->set('filebeam.filesystems.environment', null);
});

test('filestore mutations are admin only and never audit S3 secrets', function () {
    $manager = app(ManageFilestore::class);
    $user = User::factory()->create();

    expect(fn () => $manager->create($user, ['name' => 'S3', 'source' => 'database', 'driver' => 's3', 'placement_enabled' => true, 'configuration' => ['key' => 'access-key', 'secret' => 'secret-value', 'region' => 'us-east-1', 'bucket' => 'filebeam-private']]))
        ->toThrow(AuthorizationException::class);

    $store = $manager->create(filestoreAdministrator(), ['name' => 'S3', 'source' => 'database', 'driver' => 's3', 'placement_enabled' => true, 'configuration' => ['key' => 'access-key', 'secret' => 'secret-value', 'region' => 'us-east-1', 'bucket' => 'filebeam-private']]);
    $audit = AdminAudit::query()->where('action', 'filestore.created')->firstOrFail();

    expect($store->configuration['secret'])->toBe('secret-value')
        ->and(json_encode($audit->changes))->not->toContain('secret-value')
        ->and(json_encode($audit->changes))->toContain('[redacted]');
});

test('environment management locks every filestore mutation', function () {
    $registry = Mockery::mock(FilestoreRegistry::class);
    $registry->shouldReceive('environmentManaged')->andReturn(true);
    app()->instance(FilestoreRegistry::class, $registry);
    $manager = app(ManageFilestore::class);
    $admin = filestoreAdministrator();

    expect(fn () => $manager->create($admin, ['name' => 'Local', 'source' => 'database', 'driver' => 'local', 'placement_enabled' => true, 'configuration' => ['root' => 'transfers']]))
        ->toThrow(AuthorizationException::class);
});

test('former environment stores can be managed after the environment lock is removed', function (): void {
    $admin = filestoreAdministrator();
    config()->set('filebeam.filesystems.environment', ['transfers']);
    $store = Filestore::factory()->create(['source' => 'environment', 'disk_name' => 'transfers']);
    $plan = Plan::factory()->create();
    $plan->filestores()->attach($store, ['is_default' => true]);

    config()->set('filebeam.filesystems.environment', null);
    $this->actingAs($admin, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(EditFilestore::class, ['record' => $store->getKey()])
        ->fillForm(['name' => 'Former environment store', 'placement_enabled' => false])
        ->call('save')
        ->assertHasNoFormErrors();

    expect($store->refresh()->id)->toBe($store->id)
        ->and($store->name)->toBe('Former environment store')
        ->and($store->source)->toBe('environment')
        ->and($store->disk_name)->toBe('transfers')
        ->and($store->placement_enabled)->toBeFalse();

    config()->set('filebeam.filesystems.environment', ['transfers']);
    expect(fn () => app(ManageFilestore::class)->update($admin, $store, [
        'name' => 'Blocked again',
        'placement_enabled' => true,
    ]))->toThrow(AuthorizationException::class);

    config()->set('filebeam.filesystems.environment', null);
    $plan->filestores()->detach($store);
    app(ManageFilestore::class)->delete($admin, $store);

    expect(Filestore::query()->whereKey($store->id)->exists())->toBeFalse();
});

test('a store selected by a pending transfer cannot be deleted', function () {
    $admin = filestoreAdministrator();
    $store = Filestore::factory()->create();
    Transfer::factory()->create(['filestore_ids' => [$store->id], 'status' => TransferStatus::Pending]);

    expect(fn () => app(ManageFilestore::class)->delete($admin, $store))->toThrow(ValidationException::class);
    expect(Filestore::query()->whereKey($store->id)->exists())->toBeTrue();
});

test('environment-managed plans require allowed and default stores from the environment pool', function () {
    $admin = filestoreAdministrator();
    config()->set('filebeam.filesystems.environment', ['first', 'second']);
    $first = Filestore::factory()->create(['name' => 'First', 'source' => 'environment', 'disk_name' => 'first']);
    $second = Filestore::factory()->create(['name' => 'Second', 'source' => 'environment', 'disk_name' => 'second']);
    $excluded = Filestore::factory()->create(['name' => 'Excluded', 'disk_name' => 'excluded']);
    $plan = Plan::factory()->create();
    $attributes = [
        'maximum_transfer_bytes' => $plan->maximum_transfer_bytes,
        'maximum_file_count' => $plan->maximum_file_count,
        'maximum_note_bytes' => $plan->maximum_note_bytes,
        'default_file_retention_hours' => $plan->default_file_retention_hours,
        'maximum_file_retention_hours' => $plan->maximum_file_retention_hours,
        'default_note_retention_hours' => $plan->default_note_retention_hours,
        'maximum_note_retention_hours' => $plan->maximum_note_retention_hours,
        'is_active' => true,
        'placement_mode' => 'replicate',
        'filestore_ids' => [$first->id, $excluded->id],
        'default_filestore_ids' => [$first->id],
    ];

    expect(fn () => app(ManagePlan::class)->update($admin, $plan, $attributes))->toThrow(ValidationException::class);

    $attributes['filestore_ids'] = [$first->id, $second->id];
    $attributes['default_filestore_ids'] = [$first->id, $second->id];
    app(ManagePlan::class)->update($admin, $plan, $attributes);

    expect($plan->refresh()->placement_mode)->toBe('replicate')
        ->and((bool) $plan->filestores()->whereKey($first->id)->firstOrFail()->pivot->is_default)->toBeTrue()
        ->and((bool) $plan->filestores()->whereKey($second->id)->firstOrFail()->pivot->is_default)->toBeTrue()
        ->and(AdminAudit::query()->where('action', 'plan.updated')->latest()->value('changes'))->toMatchArray([
            'filestores' => [
                'from' => ['ids' => [], 'default_ids' => []],
                'to' => ['ids' => [$first->id, $second->id], 'default_ids' => [$first->id, $second->id]],
            ],
        ]);
});

test('a pending transfer selection prevents changes to a store identity', function (): void {
    $admin = filestoreAdministrator();
    $store = app(ManageFilestore::class)->create($admin, [
        'name' => 'Local',
        'source' => 'database',
        'driver' => 'local',
        'placement_enabled' => true,
        'configuration' => ['root' => 'transfers'],
    ]);
    Transfer::factory()->create(['filestore_ids' => [$store->id], 'status' => TransferStatus::Pending]);

    expect(fn () => app(ManageFilestore::class)->update($admin, $store, [
        'name' => 'Local',
        'placement_enabled' => true,
        'configuration' => ['root' => 'replacement'],
    ]))->toThrow(ValidationException::class);
});

test('administrators can view the filestore workspace', function () {
    $admin = filestoreAdministrator();
    $store = Filestore::factory()->create();

    $this->actingAs($admin, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    expect(FilestoreResource::canViewAny())->toBeTrue();
    Livewire::test(ListFilestores::class)->assertCanSeeTableRecords([$store]);
});

test('edit form does not hydrate stored S3 credentials', function (): void {
    $admin = filestoreAdministrator();
    $store = Filestore::factory()->create([
        'source' => 'database',
        'disk_name' => null,
        'driver' => 's3',
        'configuration' => ['key' => 'access-key', 'secret' => 'secret-value', 'region' => 'us-east-1', 'bucket' => 'filebeam-private'],
    ]);

    $this->actingAs($admin, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(EditFilestore::class, ['record' => $store->getKey()])
        ->assertSet('data.configuration.key', null)
        ->assertSet('data.configuration.secret', null);
});

test('administrators create local stores and rotate S3 credentials without changing identity settings', function (): void {
    $admin = filestoreAdministrator();
    $s3 = Filestore::factory()->create([
        'source' => 'database',
        'disk_name' => null,
        'driver' => 's3',
        'configuration' => ['key' => 'old-key', 'secret' => 'old-secret', 'region' => 'us-east-1', 'bucket' => 'filebeam-private', 'endpoint' => 'https://s3.example.test'],
    ]);

    $this->actingAs($admin, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(CreateFilestore::class)
        ->fillForm([
            'name' => 'Local uploads',
            'source' => 'database',
            'driver' => 'local',
            'placement_enabled' => true,
            'configuration' => ['root' => 'uploads'],
        ])
        ->call('create')
        ->assertHasNoFormErrors();

    Livewire::test(EditFilestore::class, ['record' => $s3->getKey()])
        ->fillForm(['name' => 'S3 uploads', 'placement_enabled' => false, 'configuration' => ['key' => 'new-key', 'secret' => 'new-secret']])
        ->call('save')
        ->assertHasNoFormErrors();

    expect(Filestore::query()->where('name', 'Local uploads')->value('configuration'))->toMatchArray(['root' => 'uploads'])
        ->and($s3->refresh()->configuration)->toMatchArray(['key' => 'new-key', 'secret' => 'new-secret', 'region' => 'us-east-1', 'bucket' => 'filebeam-private', 'endpoint' => 'https://s3.example.test'])
        ->and($s3->placement_enabled)->toBeFalse();
});

test('store identity changes submitted by a client are rejected', function (): void {
    $admin = filestoreAdministrator();
    $store = Filestore::factory()->create([
        'source' => 'database',
        'disk_name' => null,
        'driver' => 's3',
        'configuration' => ['key' => 'access-key', 'secret' => 'secret-value', 'region' => 'us-east-1', 'bucket' => 'filebeam-private'],
    ]);

    expect(fn () => app(ManageFilestore::class)->update($admin, $store, [
        'name' => $store->name,
        'placement_enabled' => true,
        'configuration' => ['bucket' => 'replacement-bucket'],
    ]))->toThrow(ValidationException::class);
});

test('filestore list and view use populated database aggregates', function (): void {
    $admin = filestoreAdministrator();
    $store = Filestore::factory()->create(['name' => 'Measured store']);
    $plan = Plan::factory()->create(['name' => 'Default plan']);
    $plan->filestores()->attach($store, ['is_default' => true]);
    $inactivePlan = Plan::factory()->create(['name' => 'Archived plan', 'is_active' => false]);
    $inactivePlan->filestores()->attach($store, ['is_default' => false]);
    $transfer = Transfer::factory()->create();
    $firstItem = TransferItem::factory()->for($transfer)->create();
    $secondItem = TransferItem::factory()->for($transfer)->create(['position' => 1]);
    TransferChunkLocation::factory()->for(TransferChunk::factory()->for($firstItem, 'item'), 'chunk')->for($store)->create(['ciphertext_bytes' => 100]);
    TransferChunkLocation::factory()->for(TransferChunk::factory()->for($secondItem, 'item'), 'chunk')->for($store)->create(['ciphertext_bytes' => 200]);
    $noteTransfer = Transfer::factory()->create(['kind' => TransferKind::Note]);
    $noteItem = TransferItem::factory()->for($noteTransfer)->create();
    TransferChunkLocation::factory()->for(TransferChunk::factory()->for($noteItem, 'item'), 'chunk')->for($store)->create(['ciphertext_bytes' => 50]);
    TransferChunkUpload::factory()->for($store)->create(['ciphertext_bytes' => 300, 'valid_until' => now()->addHour()]);

    $measured = FilestoreResource::getEloquentQuery()->findOrFail($store->id);

    expect($measured->file_items_count)->toBe(2)
        ->and($measured->note_items_count)->toBe(1)
        ->and($measured->locations_count)->toBe(3)
        ->and($measured->physical_bytes)->toBe(350)
        ->and($measured->upload_attempts_count)->toBe(1)
        ->and($measured->attempt_bytes)->toBe(300)
        ->and($measured->assigned_plans_count)->toBe(2);

    $this->actingAs($admin, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListFilestores::class)->assertCanSeeTableRecords([$store])->assertSee('Measured store');
    Livewire::test(ViewFilestore::class, ['record' => $store->getKey()])
        ->assertSee('Files')
        ->assertSee('Notes')
        ->assertSee('Default plan (default)')
        ->assertSee('Archived plan (inactive)');
});
