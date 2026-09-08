<?php

declare(strict_types=1);

use App\Enums\ReportStatus;
use App\Enums\TransferKind;
use App\Enums\TransferStatus;
use App\Models\AdminAudit;
use App\Models\FileReport;
use App\Models\Transfer;
use App\Models\TransferChunk;
use App\Models\TransferChunkLocation;
use App\Models\TransferItem;
use App\Models\TransferKeyEnvelope;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;

function tableHasIndexForColumns(string $table, array $columns): bool
{
    return collect(Schema::getIndexes($table))
        ->contains(fn (array $index): bool => $index['columns'] === $columns);
}

test('operational indexes retain their explicit column order', function () {
    expect(tableHasIndexForColumns('users', ['plan_id']))->toBeTrue()
        ->and(tableHasIndexForColumns('transfers', ['plan_id']))->toBeTrue()
        ->and(tableHasIndexForColumns('transfers', ['status', 'created_at']))->toBeTrue()
        ->and(tableHasIndexForColumns('transfers', ['status', 'updated_at']))->toBeTrue()
        ->and(tableHasIndexForColumns('transfers', ['expires_at']))->toBeTrue()
        ->and(tableHasIndexForColumns('transfers', ['created_at']))->toBeTrue()
        ->and(tableHasIndexForColumns('transfer_key_envelopes', ['account_key_bundle_id']))->toBeTrue()
        ->and(tableHasIndexForColumns('file_reports', ['transfer_id', 'status']))->toBeTrue()
        ->and(tableHasIndexForColumns('file_reports', ['reporter_id']))->toBeTrue()
        ->and(tableHasIndexForColumns('file_reports', ['created_at']))->toBeTrue()
        ->and(tableHasIndexForColumns('admin_audits', ['target_type', 'target_id', 'created_at']))->toBeTrue()
        ->and(tableHasIndexForColumns('admin_audits', ['created_at']))->toBeTrue()
        ->and(tableHasIndexForColumns('admin_audits', ['target_type', 'target_id']))->toBeFalse();
});

test('json booleans enums and big integers round trip through the database', function () {
    $transfer = Transfer::factory()->create([
        'kind' => TransferKind::Note,
        'status' => TransferStatus::Available,
        'burn_on_read' => true,
        'declared_ciphertext_bytes' => 4_294_967_296,
        'ciphertext_bytes' => 4_294_967_295,
    ]);
    $report = FileReport::factory()->for($transfer)->create(['status' => ReportStatus::InReview]);
    $audit = AdminAudit::factory()->create([
        'target_id' => $transfer->id,
        'changes' => [
            'enabled' => true,
            'disabled' => false,
            'previous' => null,
            'bytes' => 4_294_967_296,
            'metadata' => ['source' => 'compatibility', 'labels' => ["\u{00e9}", 'ASCII']],
        ],
    ]);

    $transfer = $transfer->fresh();
    $report = $report->fresh();
    $changes = $audit->fresh()->changes;

    expect($transfer->kind)->toBe(TransferKind::Note)
        ->and($transfer->status)->toBe(TransferStatus::Available)
        ->and($transfer->burn_on_read)->toBeTrue()
        ->and($transfer->declared_ciphertext_bytes)->toBe(4_294_967_296)
        ->and($transfer->ciphertext_bytes)->toBe(4_294_967_295)
        ->and($report->status)->toBe(ReportStatus::InReview)
        ->and($changes['enabled'])->toBeTrue()
        ->and($changes['disabled'])->toBeFalse()
        ->and($changes['previous'])->toBeNull()
        ->and($changes['bytes'])->toBe(4_294_967_296)
        ->and($changes['metadata']['source'])->toBe('compatibility')
        ->and($changes['metadata']['labels'])->toBe(["\u{00e9}", 'ASCII']);

    expect(Transfer::query()->where('burn_on_read', true)->where('status', TransferStatus::Available)->count())->toBe(1)
        ->and(Transfer::query()->where('burn_on_read', false)->count())->toBe(0);
});

test('original migrations can be reset and recreated on sqlite', function () {
    config()->set('database.connections.sqlite_lifecycle', array_replace(config('database.connections.sqlite'), [
        'database' => ':memory:',
        'url' => null,
    ]));

    try {
        $this->artisan('migrate', ['--database' => 'sqlite_lifecycle', '--force' => true])->assertSuccessful();
        expect(Schema::connection('sqlite_lifecycle')->hasTable('report_notes'))->toBeTrue();

        $this->artisan('migrate:reset', ['--database' => 'sqlite_lifecycle', '--force' => true])->assertSuccessful();
        expect(Schema::connection('sqlite_lifecycle')->hasTable('transfers'))->toBeFalse()
            ->and(Schema::connection('sqlite_lifecycle')->hasTable('users'))->toBeFalse();

        $this->artisan('migrate', ['--database' => 'sqlite_lifecycle', '--force' => true])->assertSuccessful();
        expect(Schema::connection('sqlite_lifecycle')->hasTable('transfer_chunks'))->toBeTrue()
            ->and(Schema::connection('sqlite_lifecycle')->hasTable('report_notes'))->toBeTrue();
    } finally {
        DB::purge('sqlite_lifecycle');
        config()->set('database.connections.sqlite_lifecycle', null);
    }
});

test('deleting a transfer cascades its ciphertext records and preserves reports', function () {
    $transfer = Transfer::factory()->create();
    $item = TransferItem::factory()->for($transfer)->create();
    $chunk = TransferChunk::factory()->for($item, 'item')->create();
    $location = TransferChunkLocation::factory()->for($chunk, 'chunk')->create();
    $envelope = TransferKeyEnvelope::factory()->for($transfer)->create();
    $report = FileReport::factory()->for($transfer)->create();

    $transfer->delete();

    $this->assertDatabaseMissing('transfer_items', ['id' => $item->id]);
    $this->assertDatabaseMissing('transfer_chunks', ['id' => $chunk->id]);
    $this->assertDatabaseMissing('transfer_chunk_locations', ['id' => $location->id]);
    $this->assertDatabaseMissing('transfer_key_envelopes', ['id' => $envelope->id]);
    expect($report->fresh()->transfer_id)->toBeNull();
});
