<?php

declare(strict_types=1);

use App\Enums\TransferStatus;
use App\Models\Transfer;
use App\Models\TransferItem;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Redis;
use Illuminate\Support\Str;

beforeEach(function (): void {
    $host = getenv('FILEBEAM_TEST_REDIS_HOST');

    if (! is_string($host) || $host === '') {
        $this->markTestSkipped('Set FILEBEAM_TEST_REDIS_HOST to run Redis integration tests.');
    }

    $port = (int) (getenv('FILEBEAM_TEST_REDIS_PORT') ?: 6379);
    $this->redisPrefix = 'filebeam:redis-integration:'.Str::lower(Str::random(16)).':';
    config()->set([
        'cache.default' => 'redis',
        'cache.prefix' => $this->redisPrefix,
        'cache.stores.redis.connection' => 'cache',
        'cache.stores.redis.lock_connection' => 'cache',
        'database.redis.options.prefix' => $this->redisPrefix,
        'database.redis.cache.host' => $host,
        'database.redis.cache.port' => $port,
        'database.redis.cache.database' => 15,
    ]);
    $this->app->forgetInstance('redis');
    Cache::purge('redis');

    $this->uploadToken = Str::random(64);
    $this->monitorToken = Str::random(64);
    $this->transfer = Transfer::factory()->create([
        'status' => TransferStatus::Pending,
        'encrypted_descriptor' => 'descriptor',
        'declared_ciphertext_bytes' => 10,
        'item_count' => 1,
        'upload_token_hash' => hash('sha256', $this->uploadToken),
        'monitor_token_hash' => hash('sha256', $this->monitorToken),
        'expires_at' => now()->addHour(),
    ]);
    $this->item = TransferItem::factory()->for($this->transfer)->create([
        'declared_ciphertext_bytes' => 10,
    ]);
});

test('turbo uploader state persists across Redis repositories with its bounded key TTL', function (): void {
    $pulseKey = "filebeam:turbo:{$this->transfer->id}:pulse";

    $this->patchJson("/api/v1/transfers/{$this->transfer->id}/progress", [], [
        'X-Filebeam-Upload-Token' => $this->uploadToken,
    ])->assertNoContent();

    expect(Redis::connection('cache')->ttl($this->redisPrefix.$pulseKey))->toBeGreaterThan(0)
        ->toBeLessThanOrEqual(31);

    Cache::purge('redis');

    expect(Cache::store('redis')->get($pulseKey))->toBeTrue();
    $this->getJson("/api/v1/transfers/{$this->transfer->id}/progress")
        ->assertOk()
        ->assertJsonPath('data.uploader_status', 'uploading');
})->group('redis');

test('turbo session APIs contend on a Redis lock and retain token-authorized updates', function (): void {
    $lock = Cache::lock("filebeam:turbo:{$this->transfer->id}:sessions:lock", 5);
    expect($lock->get())->toBeTrue();

    $this->postJson("/api/v1/transfers/{$this->transfer->id}/download-sessions", [
        'item_ids' => [$this->item->id],
    ])->assertServiceUnavailable();
    $lock->release();

    $session = $this->postJson("/api/v1/transfers/{$this->transfer->id}/download-sessions", [
        'item_ids' => [$this->item->id],
    ])->assertCreated()->json('data');
    $this->patchJson("/api/v1/transfers/{$this->transfer->id}/download-sessions/{$session['id']}", [
        'sequence' => 1,
        'progress' => 25,
        'status' => 'downloading',
    ], ['X-Filebeam-Session-Token' => $session['token']])->assertNoContent();

    Cache::purge('redis');

    $this->getJson("/api/v1/transfers/{$this->transfer->id}/monitor", [
        'X-Filebeam-Monitor-Token' => $this->monitorToken,
    ])->assertOk()
        ->assertJsonPath('data.sessions.0.id', $session['id'])
        ->assertJsonPath('data.sessions.0.progress', 25)
        ->assertJsonPath('data.sessions.0.status', 'downloading');
})->group('redis');
