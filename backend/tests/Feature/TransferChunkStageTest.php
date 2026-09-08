<?php

declare(strict_types=1);

use App\Models\Filestore;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferChunkStage;
use App\Support\ChunkStaging;
use App\Support\FilestoreRegistry;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Contracts\Filesystem\Filesystem;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\Storage;

beforeEach(function (): void {
    $this->testStoragePath = sys_get_temp_dir().'/filebeam-stage-tests-'.bin2hex(random_bytes(8));
    $this->originalStoragePath = app()->storagePath();
    $this->originalPublicPath = app()->publicPath();
    File::ensureDirectoryExists($this->testStoragePath);
    app()->useStoragePath($this->testStoragePath);
    Plan::factory()->create(['slug' => 'default']);
    $this->seed(FilestoreSeeder::class);
    Storage::fake('transfers');
    config()->set('filebeam.staging.root', storage_path('framework/testing/transfer-staging'));
    File::deleteDirectory(config('filebeam.staging.root'));
});

afterEach(function (): void {
    app()->useStoragePath($this->originalStoragePath);
    app()->usePublicPath($this->originalPublicPath);
    File::deleteDirectory($this->testStoragePath);
});

function stagedTransfer(int $ciphertextBytes = 16): array
{
    $response = test()->postJson('/api/v1/transfers', [
        'kind' => 'files', 'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => $ciphertextBytes, 'chunk_count' => 1]],
    ])->assertCreated();

    return [$response->json('data.id'), $response->json('data.items.0.id'), $response->json('data.upload_token')];
}

function stagePath(string $upload): string
{
    return rtrim((string) config('filebeam.staging.root'), '/').'/'.$upload.'.bin';
}

function beginStage(string $transfer, string $item, string $token, string $upload, string $ciphertext): string
{
    $path = "/api/v1/transfers/{$transfer}/items/{$item}/chunks/0/uploads/{$upload}";
    test()->putJson($path, ['ciphertext_bytes' => strlen($ciphertext), 'checksum' => hash('sha256', $ciphertext)], ['X-Filebeam-Upload-Token' => $token])->assertCreated();

    return $path;
}

test('staged parts publish the normal full ciphertext chunk', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789abc';
    $path = "/api/v1/transfers/{$transfer}/items/{$item}/chunks/0/uploads/{$upload}";
    $ciphertext = str_repeat('x', 16);

    $this->putJson($path, ['ciphertext_bytes' => 16, 'checksum' => hash('sha256', $ciphertext)], ['X-Filebeam-Upload-Token' => $token])
        ->assertCreated()->assertJsonPath('data.state', 'receiving')->assertJsonPath('data.offset', 0);
    $this->call('PUT', $path.'/parts/0', server: ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_X_FILEBEAM_PART_CHECKSUM' => hash('sha256', $ciphertext)], content: $ciphertext)
        ->assertCreated()->assertJsonPath('data.offset', 16);
    $this->postJson($path.'/complete', [], ['X-Filebeam-Upload-Token' => $token])
        ->assertOk()->assertJsonPath('data.state', 'complete');
    $this->postJson("/api/v1/transfers/{$transfer}/complete", ['encrypted_manifest' => 'manifest'], ['X-Filebeam-Upload-Token' => $token])
        ->assertOk();
    $this->get("/api/v1/transfers/{$transfer}/items/{$item}/chunks/0")->assertOk()->assertStreamedContent($ciphertext);
    expect(Transfer::query()->findOrFail($transfer)->chunks()->count())->toBe(1);
});

test('staged parts are exactly idempotent and reject conflicting offsets', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789abd';
    $path = "/api/v1/transfers/{$transfer}/items/{$item}/chunks/0/uploads/{$upload}";
    $body = str_repeat('x', 16);
    $this->putJson($path, ['ciphertext_bytes' => 16, 'checksum' => hash('sha256', $body)], ['X-Filebeam-Upload-Token' => $token])->assertCreated();
    $headers = ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_X_FILEBEAM_PART_CHECKSUM' => hash('sha256', $body)];
    $this->call('PUT', $path.'/parts/0', server: $headers, content: $body)->assertCreated();
    $this->call('PUT', $path.'/parts/0', server: $headers, content: $body)->assertCreated()->assertJsonPath('data.offset', 16);
    $this->call('PUT', $path.'/parts/1', server: $headers, content: $body)->assertConflict();
});

test('expired or missing staging is not acknowledged', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789abe';
    $path = "/api/v1/transfers/{$transfer}/items/{$item}/chunks/0/uploads/{$upload}";
    $ciphertext = str_repeat('x', 16);
    $this->putJson($path, ['ciphertext_bytes' => 16, 'checksum' => hash('sha256', $ciphertext)], ['X-Filebeam-Upload-Token' => $token])->assertCreated();
    TransferChunkStage::query()->whereKey($upload)->update(['expires_at' => now()->subSecond()]);
    $this->getJson($path, ['X-Filebeam-Upload-Token' => $token])->assertGone();
});

test('unknown staging identifiers are gone rather than published chunk probes', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $path = "/api/v1/transfers/{$transfer}/items/{$item}/chunks/0/uploads/018f7b8c-2f2d-4f2d-8c43-123456789ab0";

    $this->getJson($path, ['X-Filebeam-Upload-Token' => $token])->assertGone();
});

test('a lost receiving file is gone and cannot be recreated by a part request', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ab1';
    $path = beginStage($transfer, $item, $token, $upload, str_repeat('x', 16));
    File::delete(stagePath($upload));

    $this->call('PUT', $path.'/parts/0', server: ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_X_FILEBEAM_PART_CHECKSUM' => hash('sha256', str_repeat('x', 16))], content: str_repeat('x', 16))->assertGone();
    expect(File::exists(stagePath($upload)))->toBeFalse();
});

test('bad part checksums and undersized non-final parts are rejected', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ab2';
    $path = beginStage($transfer, $item, $token, $upload, str_repeat('x', 16));

    $this->call('PUT', $path.'/parts/0', server: ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_X_FILEBEAM_PART_CHECKSUM' => str_repeat('0', 64)], content: str_repeat('x', 16))->assertUnprocessable();
});

test('completion rejects gaps and checksum mismatches', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ab3';
    $path = beginStage($transfer, $item, $token, $upload, str_repeat('x', 16));
    $this->postJson($path.'/complete', [], ['X-Filebeam-Upload-Token' => $token])->assertConflict();
    $body = str_repeat('y', 16);
    $this->call('PUT', $path.'/parts/0', server: ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_X_FILEBEAM_PART_CHECKSUM' => hash('sha256', $body)], content: $body)->assertCreated();
    $this->postJson($path.'/complete', [], ['X-Filebeam-Upload-Token' => $token])->assertUnprocessable();
});

test('capacity includes expired rows until their files are removed', function (): void {
    config()->set('filebeam.staging.global_bytes', 32);
    [$transfer, $item, $token] = stagedTransfer();
    $first = '018f7b8c-2f2d-4f2d-8c43-123456789ab4';
    beginStage($transfer, $item, $token, $first, str_repeat('x', 16));
    TransferChunkStage::query()->whereKey($first)->update(['expires_at' => now()->subSecond()]);
    $second = '018f7b8c-2f2d-4f2d-8c43-123456789ab5';
    $this->putJson("/api/v1/transfers/{$transfer}/items/{$item}/chunks/0/uploads/{$second}", ['ciphertext_bytes' => 16, 'checksum' => hash('sha256', str_repeat('y', 16))], ['X-Filebeam-Upload-Token' => $token])->assertStatus(503);
});

test('a staging root symlinked into the public directory is rejected', function (): void {
    $publicPath = $this->testStoragePath.'/public';
    app()->usePublicPath($publicPath);
    File::makeDirectory($publicPath, 0700, true);
    symlink($publicPath, $this->testStoragePath.'/staging-alias');
    config()->set('filebeam.staging.root', $this->testStoragePath.'/staging-alias');
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ab5';

    $this->putJson("/api/v1/transfers/{$transfer}/items/{$item}/chunks/0/uploads/{$upload}", ['ciphertext_bytes' => 16, 'checksum' => hash('sha256', str_repeat('x', 16))], ['X-Filebeam-Upload-Token' => $token])->assertStatus(503);
    expect(File::glob($publicPath.'/*.bin'))->toBeEmpty();
});

test('stage row limits bound new sessions', function (): void {
    config()->set('filebeam.staging.global_max_rows', 1);
    [$transfer, $item, $token] = stagedTransfer();
    beginStage($transfer, $item, $token, '018f7b8c-2f2d-4f2d-8c43-123456789ab6', str_repeat('x', 16));
    [$otherTransfer, $otherItem, $otherToken] = stagedTransfer();
    $this->putJson("/api/v1/transfers/{$otherTransfer}/items/{$otherItem}/chunks/0/uploads/018f7b8c-2f2d-4f2d-8c43-123456789ab7", ['ciphertext_bytes' => 16, 'checksum' => hash('sha256', str_repeat('z', 16))], ['X-Filebeam-Upload-Token' => $otherToken])->assertStatus(503);
});

test('wrong upload token cannot inspect a stage', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $path = beginStage($transfer, $item, $token, '018f7b8c-2f2d-4f2d-8c43-123456789ab8', str_repeat('x', 16));
    $this->getJson($path, ['X-Filebeam-Upload-Token' => 'wrong'])->assertForbidden();
});

test('finalizing state recovers to receiving when no complete replica exists', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ab9';
    $path = beginStage($transfer, $item, $token, $upload, str_repeat('x', 16));
    TransferChunkStage::query()->whereKey($upload)->update(['state' => 'finalizing']);

    $this->getJson($path, ['X-Filebeam-Upload-Token' => $token])->assertCreated()->assertJsonPath('data.state', 'receiving');
});

test('destroy removes staged data and prevents future parts', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789aca';
    $path = beginStage($transfer, $item, $token, $upload, str_repeat('x', 16));
    $this->deleteJson($path, [], ['X-Filebeam-Upload-Token' => $token])->assertNoContent();
    $this->getJson($path, ['X-Filebeam-Upload-Token' => $token])->assertGone();
    expect(File::exists(stagePath($upload)))->toBeFalse();
});

test('pruning expired stages removes their row and private file', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789acb';
    beginStage($transfer, $item, $token, $upload, str_repeat('x', 16));
    TransferChunkStage::query()->whereKey($upload)->update(['expires_at' => now()->subSecond()]);

    app(ChunkStaging::class)->prune();
    expect(TransferChunkStage::query()->find($upload))->toBeNull();
    expect(File::exists(stagePath($upload)))->toBeFalse();
});

test('finalizing state becomes complete when all replicas were already published', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789acc';
    $body = str_repeat('x', 16);
    $path = beginStage($transfer, $item, $token, $upload, $body);
    $this->call('PUT', $path.'/parts/0', server: ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_X_FILEBEAM_PART_CHECKSUM' => hash('sha256', $body)], content: $body)->assertCreated();
    $this->postJson($path.'/complete', [], ['X-Filebeam-Upload-Token' => $token])->assertOk();
    TransferChunkStage::query()->whereKey($upload)->update(['state' => 'finalizing']);

    $this->getJson($path, ['X-Filebeam-Upload-Token' => $token])->assertOk()->assertJsonPath('data.state', 'complete');
});

test('a direct upload prevents a staged upload with different ciphertext', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $chunkPath = "/api/v1/transfers/{$transfer}/items/{$item}/chunks/0";
    $body = str_repeat('x', 16);
    $this->call('PUT', $chunkPath, server: ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_CONTENT_LENGTH' => '16'], content: $body)->assertCreated();

    $this->putJson("{$chunkPath}/uploads/018f7b8c-2f2d-4f2d-8c43-123456789acd", ['ciphertext_bytes' => 16, 'checksum' => hash('sha256', str_repeat('y', 16))], ['X-Filebeam-Upload-Token' => $token])->assertConflict();
});

test('a completed stage releases capacity only after its local file is removed', function (): void {
    config()->set('filebeam.staging.global_bytes', 32);
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ace';
    $body = str_repeat('x', 16);
    $path = beginStage($transfer, $item, $token, $upload, $body);
    $this->call('PUT', $path.'/parts/0', server: ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_X_FILEBEAM_PART_CHECKSUM' => hash('sha256', $body)], content: $body)->assertCreated();
    $this->postJson($path.'/complete', [], ['X-Filebeam-Upload-Token' => $token])->assertOk();
    expect(TransferChunkStage::query()->findOrFail($upload)->released_at)->not->toBeNull();

    [$otherTransfer, $otherItem, $otherToken] = stagedTransfer();
    beginStage($otherTransfer, $otherItem, $otherToken, '018f7b8c-2f2d-4f2d-8c43-123456789acf', str_repeat('y', 16));
});

test('multi-part uploads validate every acknowledged prefix before resuming', function (): void {
    config()->set('filebeam.transfers.chunk_bytes', 131_072);
    config()->set('filebeam.staging.part_max_bytes', 65_536);
    $ciphertext = str_repeat('a', 65_536).str_repeat('b', 65_536).str_repeat('c', 16);
    [$transfer, $item, $token] = stagedTransfer(strlen($ciphertext));
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ad0';
    $path = beginStage($transfer, $item, $token, $upload, $ciphertext);
    foreach ([0 => substr($ciphertext, 0, 65_536), 65_536 => substr($ciphertext, 65_536, 65_536), 131_072 => substr($ciphertext, 131_072)] as $offset => $body) {
        $this->call('PUT', $path.'/parts/'.$offset, server: ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_X_FILEBEAM_PART_CHECKSUM' => hash('sha256', $body)], content: $body)->assertCreated();
    }
    $this->postJson($path.'/complete', [], ['X-Filebeam-Upload-Token' => $token])->assertOk();
});

test('truncated or corrupted acknowledged data is gone instead of being zero-filled or duplicated', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ad1';
    $body = str_repeat('x', 16);
    $path = beginStage($transfer, $item, $token, $upload, $body);
    $headers = ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_X_FILEBEAM_PART_CHECKSUM' => hash('sha256', $body)];
    $this->call('PUT', $path.'/parts/0', server: $headers, content: $body)->assertCreated();
    $handle = fopen(stagePath($upload), 'rb+');
    ftruncate($handle, 8);
    fclose($handle);
    $this->getJson($path, ['X-Filebeam-Upload-Token' => $token])->assertGone();
    $this->call('PUT', $path.'/parts/0', server: $headers, content: $body)->assertGone();
});

test('a published direct chunk with a different checksum conflicts with its stage', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ad2';
    $path = beginStage($transfer, $item, $token, $upload, str_repeat('x', 16));
    $this->call('PUT', "/api/v1/transfers/{$transfer}/items/{$item}/chunks/0", server: ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_CONTENT_LENGTH' => '16'], content: str_repeat('y', 16))->assertCreated();
    $this->getJson($path, ['X-Filebeam-Upload-Token' => $token])->assertConflict();
});

test('the reaper rechecks a refreshed stage after acquiring its lock', function (): void {
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ad3';
    beginStage($transfer, $item, $token, $upload, str_repeat('x', 16));
    $stage = TransferChunkStage::query()->findOrFail($upload);
    $stage->update(['expires_at' => now()->subSecond()]);
    $lock = app(ChunkStaging::class)->lock($stage);
    flock($lock, LOCK_EX);
    TransferChunkStage::query()->whereKey($upload)->update(['expires_at' => now()->addHour()]);
    flock($lock, LOCK_UN);
    fclose($lock);
    app(ChunkStaging::class)->prune();
    expect(TransferChunkStage::query()->find($upload))->not->toBeNull();
});

test('a staged replicated upload resumes after one destination fails', function (): void {
    config()->set('filebeam.filesystems.environment', ['stage-replica-a', 'stage-replica-b']);
    Storage::fake('stage-replica-a');
    Storage::fake('stage-replica-b');
    $stores = collect(['stage-replica-a', 'stage-replica-b'])->map(fn (string $disk): Filestore => Filestore::factory()->create([
        'name' => $disk, 'disk_name' => $disk, 'source' => 'environment',
    ]));
    $plan = Plan::query()->where('slug', 'default')->sole();
    $plan->update(['placement_mode' => 'replicate']);
    $plan->filestores()->sync($stores->mapWithKeys(fn (Filestore $store): array => [$store->id => ['is_default' => true]])->all());
    [$transfer, $item, $token] = stagedTransfer();
    $upload = '018f7b8c-2f2d-4f2d-8c43-123456789ad4';
    $body = str_repeat('x', 16);
    $path = beginStage($transfer, $item, $token, $upload, $body);
    $this->call('PUT', $path.'/parts/0', server: ['HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token, 'HTTP_X_FILEBEAM_PART_CHECKSUM' => hash('sha256', $body)], content: $body)->assertCreated();
    $failed = true;
    $registry = Mockery::mock(FilestoreRegistry::class)->makePartial();
    $registry->shouldReceive('disk')->andReturnUsing(function (Filestore $store) use ($stores, &$failed): Filesystem {
        if ($store->id === $stores[1]->id && $failed) {
            $failed = false;
            $disk = Mockery::mock(Filesystem::class);
            $disk->shouldReceive('put')->once()->andReturnFalse();

            return $disk;
        }

        return Storage::disk($store->disk_name);
    });
    app()->instance(FilestoreRegistry::class, $registry);
    $this->postJson($path.'/complete', [], ['X-Filebeam-Upload-Token' => $token])->assertStatus(503);
    expect(Transfer::query()->findOrFail($transfer)->chunks()->sole()->locations)->toHaveCount(1);
    $this->postJson($path.'/complete', [], ['X-Filebeam-Upload-Token' => $token])->assertOk();
    expect(Transfer::query()->findOrFail($transfer)->chunks()->sole()->locations)->toHaveCount(2);
});
