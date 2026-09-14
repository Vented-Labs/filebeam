<?php

declare(strict_types=1);

use App\Enums\TransferDelivery;
use App\Enums\TransferStatus;
use App\Models\Filestore;
use App\Models\Transfer;
use App\Models\TransferChunk;
use App\Models\TransferChunkLocation;
use App\Models\TransferItem;
use App\Models\User;
use Illuminate\Support\Facades\Storage;

beforeEach(function (): void {
    config()->set('filebeam.filesystems.environment', null);
    Storage::fake('transfers');
});

function recoveryTransfer(array $attributes = []): array
{
    $token = 'recovery-token';
    $store = Filestore::query()->firstOrCreate(['disk_name' => 'transfers'], [
        'name' => 'Transfers', 'source' => 'laravel', 'placement_enabled' => true,
    ]);
    $transfer = Transfer::factory()->create([
        'upload_token_hash' => hash('sha256', $token),
        'filestore_ids' => [$store->id],
        'expires_at' => now()->addHour(),
        ...$attributes,
    ]);

    return [$transfer, $token, $store];
}

function recoveryChunk(Transfer $transfer, Filestore $store, int $itemPosition, int $position, string $ciphertext, bool $stored = true): TransferChunk
{
    $item = TransferItem::query()->firstOrCreate([
        'transfer_id' => $transfer->id, 'position' => $itemPosition,
    ], [
        'chunk_count' => 3, 'declared_ciphertext_bytes' => 51,
    ]);
    $chunk = TransferChunk::factory()->for($item, 'item')->create([
        'position' => $position, 'ciphertext_bytes' => strlen($ciphertext), 'checksum' => hash('sha256', $ciphertext),
    ]);
    if ($stored) {
        $location = TransferChunkLocation::factory()->for($chunk, 'chunk')->create([
            'filestore_id' => $store->id, 'storage_path' => "recovery/{$chunk->id}.bin", 'ciphertext_bytes' => strlen($ciphertext),
        ]);
        Storage::disk('transfers')->put($location->storage_path, $ciphertext);
    }

    return $chunk;
}

test('upload status requires its capability, remains available while pending, and omits incomplete replicas', function (): void {
    [$transfer, $token, $store] = recoveryTransfer();
    $published = recoveryChunk($transfer, $store, 0, 0, 'abcdefghijklmnop');
    recoveryChunk($transfer, $store, 0, 1, 'qrstuvwxyzabcdef', stored: false);
    $other = Transfer::factory()->create(['upload_token_hash' => hash('sha256', 'other-token')]);

    $url = "/api/v1/transfers/{$transfer->id}/upload-status";
    $this->getJson($url)->assertForbidden();
    $this->getJson($url, ['X-Filebeam-Upload-Token' => 'other-token'])->assertForbidden();
    $this->getJson("/api/v1/transfers/{$other->id}/upload-status", ['X-Filebeam-Upload-Token' => $token])->assertForbidden();
    $this->getJson($url, ['X-Filebeam-Upload-Token' => $token])
        ->assertOk()
        ->assertHeader('Cache-Control', 'no-store, private')
        ->assertJsonPath('data.status', 'pending')
        ->assertJsonPath('data.upload_transport.part_max_count', config('filebeam.staging.part_max_count'))
        ->assertJsonPath('data.chunks.0.item_id', $published->transfer_item_id)
        ->assertJsonPath('data.chunks.0.checksum', $published->checksum)
        ->assertJsonCount(1, 'data.chunks')
        ->assertJsonMissing(['encrypted_manifest', 'encrypted_descriptor']);
});

test('upload status only includes fully replicated chunks and enforces expiry and page limits', function (): void {
    [$transfer, $token, $first] = recoveryTransfer(['placement_mode' => 'replicate']);
    $second = Filestore::factory()->create(['disk_name' => 'recovery-replica']);
    Storage::fake('recovery-replica');
    $transfer->update(['filestore_ids' => [$first->id, $second->id]]);
    $chunk = recoveryChunk($transfer, $first, 0, 0, 'abcdefghijklmnop');
    $this->getJson("/api/v1/transfers/{$transfer->id}/upload-status", ['X-Filebeam-Upload-Token' => $token])
        ->assertJsonCount(0, 'data.chunks');
    $location = TransferChunkLocation::factory()->for($chunk, 'chunk')->create([
        'filestore_id' => $second->id, 'storage_path' => 'recovery/replica.bin', 'ciphertext_bytes' => 16,
    ]);
    Storage::disk('recovery-replica')->put($location->storage_path, 'abcdefghijklmnop');
    $this->getJson("/api/v1/transfers/{$transfer->id}/upload-status?limit=501", ['X-Filebeam-Upload-Token' => $token])->assertUnprocessable();
    $this->getJson("/api/v1/transfers/{$transfer->id}/upload-status", ['X-Filebeam-Upload-Token' => $token])
        ->assertJsonCount(1, 'data.chunks');
    $transfer->update(['expires_at' => now()->subSecond()]);
    $this->getJson("/api/v1/transfers/{$transfer->id}/upload-status", ['X-Filebeam-Upload-Token' => $token])->assertNotFound();
});

test('upload status cannot treat chunks as published without a required store', function (): void {
    [$transfer, $token, $store] = recoveryTransfer(['placement_mode' => 'replicate', 'filestore_ids' => []]);
    recoveryChunk($transfer, $store, 0, 0, 'abcdefghijklmnop');

    $this->getJson("/api/v1/transfers/{$transfer->id}/upload-status", ['X-Filebeam-Upload-Token' => $token])
        ->assertOk()->assertJsonCount(0, 'data.chunks');
});

test('upload status cursor pages immutable chunk ordering without duplication', function (): void {
    [$transfer, $token, $store] = recoveryTransfer();
    recoveryChunk($transfer, $store, 0, 0, 'aaaaaaaaaaaaaaaa');
    recoveryChunk($transfer, $store, 0, 1, 'bbbbbbbbbbbbbbbb');
    recoveryChunk($transfer, $store, 1, 0, 'cccccccccccccccc');
    $url = "/api/v1/transfers/{$transfer->id}/upload-status?limit=2";

    $first = $this->getJson($url, ['X-Filebeam-Upload-Token' => $token])->assertOk();
    $cursor = $first->json('data.next_cursor');
    expect($first->json('data.chunks'))->toHaveCount(2)->and($cursor)->toBeString();
    $second = $this->getJson("{$url}&after={$cursor}", ['X-Filebeam-Upload-Token' => $token])->assertOk();
    expect($second->json('data.chunks'))->toHaveCount(1)
        ->and($second->json('data.next_cursor'))->toBeNull()
        ->and(array_column([...$first->json('data.chunks'), ...$second->json('data.chunks')], 'checksum'))->toHaveCount(3);
    $this->getJson("{$url}&after=not-a-cursor", ['X-Filebeam-Upload-Token' => $token])->assertUnprocessable();
});

test('link chunk downloads support bounded open and suffix ranges with strong validators', function (): void {
    [$transfer, , $store] = recoveryTransfer(['status' => TransferStatus::Available]);
    $chunk = recoveryChunk($transfer, $store, 0, 0, 'abcdefghijklmnop');
    $url = "/api/v1/transfers/{$transfer->id}/items/{$chunk->transfer_item_id}/chunks/0";
    $etag = '"'.$chunk->checksum.'"';

    $bounded = $this->get($url, ['Range' => 'bytes=2-5'])->assertStatus(206)
        ->assertHeader('Accept-Ranges', 'bytes')->assertHeader('ETag', $etag)
        ->assertHeader('Content-Range', 'bytes 2-5/16')->assertHeader('Content-Length', '4');
    expect($bounded->streamedContent())->toBe('cdef')
        ->and($bounded->headers->get('Cache-Control'))->toContain('private')->and($bounded->headers->get('Cache-Control'))->toContain('no-store')
        ->and($this->get($url, ['Range' => 'bytes=2-'])->streamedContent())->toBe('cdefghijklmnop')
        ->and($this->get($url, ['Range' => 'bytes=-4'])->streamedContent())->toBe('mnop')
        ->and($this->get($url, ['Range' => 'bytes=0-999'])->streamedContent())->toBe('abcdefghijklmnop');
});

test('invalid ranges return 416 and if range only honors a matching strong etag', function (): void {
    [$transfer, , $store] = recoveryTransfer(['status' => TransferStatus::Available]);
    $chunk = recoveryChunk($transfer, $store, 0, 0, 'abcdefghijklmnop');
    $url = "/api/v1/transfers/{$transfer->id}/items/{$chunk->transfer_item_id}/chunks/0";
    $this->get($url, ['Range' => 'bytes=99-100'])->assertStatus(416)
        ->assertHeader('Content-Range', 'bytes */16')->assertHeader('Content-Length', '0');
    $this->get($url, ['Range' => 'bytes=0-1,3-4'])->assertStatus(416);
    $this->get($url, ['Range' => 'bytes='.str_repeat('9', 256).'-'])->assertStatus(416);
    $response = $this->get($url, ['Range' => 'bytes=0-1', 'If-Range' => '"different"'])->assertOk();
    $weak = $this->get($url, ['Range' => 'bytes=0-1', 'If-Range' => 'W/"'.$chunk->checksum.'"'])->assertOk();
    $matching = $this->get($url, ['Range' => 'bytes=0-1', 'If-Range' => '"'.$chunk->checksum.'"'])->assertStatus(206);
    expect($response->streamedContent())->toBe('abcdefghijklmnop')
        ->and($weak->streamedContent())->toBe('abcdefghijklmnop')
        ->and($matching->streamedContent())->toBe('ab');
});

test('range responses preserve exact 16 byte ciphertext lengths for head requests', function (): void {
    [$transfer, , $store] = recoveryTransfer(['status' => TransferStatus::Available]);
    $chunk = recoveryChunk($transfer, $store, 0, 0, 'abcdefghijklmnop');
    $url = "/api/v1/transfers/{$transfer->id}/items/{$chunk->transfer_item_id}/chunks/0";

    $this->call('HEAD', $url, server: ['HTTP_RANGE' => 'bytes=0-0'])->assertStatus(206)
        ->assertHeader('Content-Length', '1')->assertHeader('Content-Range', 'bytes 0-0/16')
        ->assertHeader('ETag', '"'.$chunk->checksum.'"');
});

test('turbo and inbox routes share range validation and replica fallback', function (): void {
    [$turbo, , $first] = recoveryTransfer(['status' => TransferStatus::Pending, 'encrypted_descriptor' => 'descriptor']);
    $second = Filestore::factory()->create(['disk_name' => 'range-fallback']);
    Storage::fake('range-fallback');
    $turbo->update(['placement_mode' => 'replicate', 'filestore_ids' => [$first->id, $second->id]]);
    $chunk = recoveryChunk($turbo, $first, 0, 0, 'abcdefghijklmnop');
    $fallback = TransferChunkLocation::factory()->for($chunk, 'chunk')->create([
        'filestore_id' => $second->id, 'storage_path' => 'range/fallback.bin', 'ciphertext_bytes' => 16,
    ]);
    Storage::disk('transfers')->put($chunk->locations()->firstOrFail()->storage_path, 'corrupt');
    Storage::disk('range-fallback')->put($fallback->storage_path, 'abcdefghijklmnop');
    $this->get("/api/v1/transfers/{$turbo->id}/items/{$chunk->transfer_item_id}/chunks/0", ['Range' => 'bytes=1-3'])
        ->assertStatus(206)->assertHeader('Content-Range', 'bytes 1-3/16');

    $recipient = User::factory()->create();
    [$inbox, , $store] = recoveryTransfer(['status' => TransferStatus::Available, 'delivery' => TransferDelivery::Inbox, 'recipient_id' => $recipient->id]);
    $inboxChunk = recoveryChunk($inbox, $store, 0, 0, 'abcdefghijklmnop');
    $this->actingAs($recipient)->get("/account/inbox/{$inbox->id}/items/{$inboxChunk->transfer_item_id}/chunks/0", ['Range' => 'bytes=-3'])
        ->assertStatus(206)->assertHeader('Content-Range', 'bytes 13-15/16');
});
