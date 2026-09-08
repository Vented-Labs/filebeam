<?php

declare(strict_types=1);

use App\Enums\TransferStatus;
use App\Jobs\DeleteTransfer;
use App\Models\Filestore;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferItem;
use App\Support\FilestoreRegistry;
use Aws\S3\S3Client;
use Illuminate\Support\Facades\File;

beforeEach(function (): void {
    foreach (['FILEBEAM_TEST_S3_ENDPOINT', 'FILEBEAM_TEST_S3_KEY', 'FILEBEAM_TEST_S3_SECRET', 'FILEBEAM_TEST_S3_BUCKET'] as $variable) {
        if (getenv($variable) === false || getenv($variable) === '') {
            $this->markTestSkipped("Set {$variable} to run S3 filestore integration tests.");
        }
    }

    $this->s3Endpoint = (string) getenv('FILEBEAM_TEST_S3_ENDPOINT');
    $this->s3Bucket = substr((string) getenv('FILEBEAM_TEST_S3_BUCKET'), 0, 40).'-'.bin2hex(random_bytes(8));
    $this->s3Key = (string) getenv('FILEBEAM_TEST_S3_KEY');
    $this->s3Secret = (string) getenv('FILEBEAM_TEST_S3_SECRET');
    $this->s3 = new S3Client([
        'version' => 'latest',
        'region' => 'us-east-1',
        'endpoint' => $this->s3Endpoint,
        'use_path_style_endpoint' => true,
        'credentials' => ['key' => $this->s3Key, 'secret' => $this->s3Secret],
    ]);
    $this->s3->createBucket(['Bucket' => $this->s3Bucket]);
    $this->s3BucketCreated = true;
    $this->localRoot = storage_path('framework/testing/s3-filestore-integration-'.bin2hex(random_bytes(8)));
    config()->set('filebeam.filesystems.environment', null);
    config()->set('filebeam.transfers.chunk_bytes', 1);
    config()->set('filesystems.disks.s3-integration-local', [
        'driver' => 'local',
        'root' => $this->localRoot,
        'visibility' => 'private',
        'throw' => true,
    ]);
});

afterEach(function (): void {
    try {
        if ($this->s3BucketCreated ?? false) {
            do {
                $objects = $this->s3->listObjectsV2(['Bucket' => $this->s3Bucket]);
                $contents = $objects['Contents'] ?? [];
                if ($contents !== []) {
                    $this->s3->deleteObjects([
                        'Bucket' => $this->s3Bucket,
                        'Delete' => ['Objects' => array_map(fn (array $object): array => ['Key' => $object['Key']], $contents)],
                    ]);
                }
            } while (($objects['IsTruncated'] ?? false) === true);

            $this->s3->deleteBucket(['Bucket' => $this->s3Bucket]);
        }
    } finally {
        if (isset($this->localRoot)) {
            File::deleteDirectory($this->localRoot);
        }
    }
});

test('uses the installed S3 adapter for database filestores and replicated transfer storage', function (): void {
    $s3Store = Filestore::factory()->create([
        'name' => 'S3 integration',
        'source' => 'database',
        'disk_name' => null,
        'driver' => 's3',
        'configuration' => [
            'key' => $this->s3Key,
            'secret' => $this->s3Secret,
            'region' => 'us-east-1',
            'bucket' => $this->s3Bucket,
            'endpoint' => $this->s3Endpoint,
            'use_path_style_endpoint' => true,
            'visibility' => 'private',
        ],
    ]);
    $localStore = Filestore::factory()->create([
        'name' => 'Local integration',
        'source' => 'laravel',
        'disk_name' => 's3-integration-local',
    ]);
    $storedConfiguration = $s3Store->getRawOriginal('configuration');

    expect($storedConfiguration)->toBeString()
        ->not->toContain($this->s3Key)
        ->not->toContain($this->s3Secret);

    $disk = app(FilestoreRegistry::class)->disk($s3Store);
    $probe = 'adapter-probe.bin';
    $ciphertext = 'ciphertext-123456';
    expect($disk->put($probe, $ciphertext))->toBeTrue()
        ->and($disk->read($probe))->toBe($ciphertext)
        ->and(hash('sha256', $disk->read($probe)))->toBe(hash('sha256', $ciphertext));
    expect($disk->delete($probe))->toBeTrue();

    $plan = Plan::factory()->create(['slug' => 'default', 'placement_mode' => 'replicate']);
    $plan->filestores()->sync([
        $localStore->id => ['is_default' => true],
        $s3Store->id => ['is_default' => true],
    ]);
    $reservation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1,
        'chunk_bytes' => 1,
        'items' => [['ciphertext_bytes' => strlen($ciphertext), 'chunk_count' => 1]],
    ])->assertCreated()->json('data');

    $this->call('PUT', "/api/v1/transfers/{$reservation['id']}/items/{$reservation['items'][0]['id']}/chunks/0", server: [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $reservation['upload_token'],
    ], content: $ciphertext)->assertCreated()->assertJsonPath('checksum', hash('sha256', $ciphertext));
    $this->postJson("/api/v1/transfers/{$reservation['id']}/complete", ['encrypted_manifest' => 'manifest'], [
        'X-Filebeam-Upload-Token' => $reservation['upload_token'],
    ])->assertOk();

    $chunk = TransferItem::query()->findOrFail($reservation['items'][0]['id'])->chunks()->sole();
    $locations = $chunk->locations()->with('filestore')->get();
    expect($locations)->toHaveCount(2)
        ->and($chunk->checksum)->toBe(hash('sha256', $ciphertext));
    foreach ($locations as $location) {
        expect(app(FilestoreRegistry::class)->disk($location->filestore)->exists($location->storage_path))->toBeTrue();
    }

    expect($this->get("/api/v1/transfers/{$reservation['id']}/items/{$reservation['items'][0]['id']}/chunks/0")
        ->assertOk()->streamedContent())->toBe($ciphertext);

    Transfer::query()->findOrFail($reservation['id'])->update(['status' => TransferStatus::Deleting]);
    (new DeleteTransfer($reservation['id']))->handle();
    expect(Transfer::query()->find($reservation['id']))->toBeNull();
    foreach ($locations as $location) {
        expect(app(FilestoreRegistry::class)->disk($location->filestore)->exists($location->storage_path))->toBeFalse();
    }
});
