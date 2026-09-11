<?php

declare(strict_types=1);

use App\Enums\TransferDriver;
use App\Enums\TransferStatus;
use App\Http\Middleware\RequireInstallation;
use App\Models\Plan;
use App\Models\Transfer;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\RateLimiter;

beforeEach(function (): void {
    $this->withoutMiddleware(RequireInstallation::class);
    Plan::factory()->create(['slug' => 'default']);
    $this->seed(FilestoreSeeder::class);
    config()->set('filebeam.instance_settings.environment.enabled_drivers', ['http', 'webrtc']);
    config()->set('filebeam.instance_settings.environment.default_driver', 'http');
    config()->set('cache.default', 'array');
    config()->set('cache.stores.array.serialize', true);
});

/** @return array<string, mixed> */
function reserveWebRtc(string $kind = 'files', bool $burn = false): array
{
    return test()->postJson('/api/v1/transfers', ['kind' => $kind, 'driver' => 'webrtc', 'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'), 'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]], 'burn_on_read' => $burn])->assertCreated()->json('data');
}

/** @param array<string, mixed> $transfer */
function publishWebRtc(array $transfer): void
{
    test()->putJson("/api/v1/transfers/{$transfer['id']}/webrtc/publish", ['encrypted_manifest' => 'encrypted-manifest'], ['X-Filebeam-Upload-Token' => $transfer['upload_token']])->assertOk();
}

/** @param array<string, mixed> $transfer @return array<string, mixed> */
function registerWebRtc(array $transfer): array
{
    return test()->postJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions", [], ['X-Filebeam-Join-Token' => $transfer['join_token']])->assertCreated()->json('data');
}

test('webrtc links are storage free and expose only lifecycle metadata', function (): void {
    $reservation = reserveWebRtc();
    expect($reservation['driver'])->toBe('webrtc')->and($reservation['upload_transport'])->toBeNull()->and($reservation)->toHaveKey('join_token');
    $transfer = Transfer::query()->findOrFail($reservation['id']);
    expect($transfer->driver)->toBe(TransferDriver::WebRtc)->and($transfer->filestore_ids)->toBe([])->and($transfer->ciphertext_bytes)->toBe(0);
    publishWebRtc($reservation);
    expect(Transfer::query()->findOrFail($reservation['id'])->status)->toBe(TransferStatus::Live);
    $this->getJson("/api/v1/transfers/{$reservation['id']}")->assertOk()->assertJsonPath('data.driver', 'webrtc')->assertJsonPath('data.encrypted_manifest', 'encrypted-manifest')->assertJsonMissing(['join_token' => $reservation['join_token']]);
});

test('webrtc signaling isolates roles and sessions and rejects HTTP routes', function (): void {
    $first = reserveWebRtc();
    publishWebRtc($first);
    $second = reserveWebRtc();
    publishWebRtc($second);
    $session = registerWebRtc($first);
    $offer = ['description' => ['type' => 'offer', 'sdp' => 'v=0']];
    $this->putJson("/api/v1/transfers/{$first['id']}/webrtc/sessions/{$session['id']}/offer", $offer, ['X-Filebeam-Session-Token' => 'wrong'])->assertForbidden();
    $this->putJson("/api/v1/transfers/{$second['id']}/webrtc/sessions/{$session['id']}/offer", $offer, ['X-Filebeam-Session-Token' => $session['token']])->assertNotFound();
    $this->putJson("/api/v1/transfers/{$first['id']}/webrtc/sessions/{$session['id']}/answer", ['description' => ['type' => 'answer', 'sdp' => 'v=0']], ['X-Filebeam-Upload-Token' => $first['upload_token']])->assertConflict();
    $this->putJson("/api/v1/transfers/{$first['id']}/webrtc/sessions/{$session['id']}/offer", $offer, ['X-Filebeam-Session-Token' => $session['token']])->assertNoContent();
    $this->getJson("/api/v1/transfers/{$first['id']}/webrtc/sessions", ['X-Filebeam-Upload-Token' => $first['upload_token']])
        ->assertJsonPath('data.sessions.0.id', $session['id'])
        ->assertJsonPath('data.sessions.0.offer.sdp', 'v=0');
    $this->putJson("/api/v1/transfers/{$first['id']}/webrtc/sessions/{$session['id']}/answer", ['description' => ['type' => 'answer', 'sdp' => 'v=0']], ['X-Filebeam-Upload-Token' => $first['upload_token']])->assertNoContent();
    $this->putJson("/api/v1/transfers/{$first['id']}/items/{$first['items'][0]['id']}/chunks/0", [], ['X-Filebeam-Upload-Token' => $first['upload_token']])->assertNotFound();
    $this->putJson("/api/v1/transfers/{$first['id']}/descriptor", ['encrypted_descriptor' => 'x'], ['X-Filebeam-Upload-Token' => $first['upload_token']])->assertNotFound();
});

test('SDP is immutable and terminal reports are exactly idempotent', function (): void {
    $transfer = reserveWebRtc();
    publishWebRtc($transfer);
    $session = registerWebRtc($transfer);
    $endpoint = "/api/v1/transfers/{$transfer['id']}/webrtc/sessions/{$session['id']}";
    $headers = ['X-Filebeam-Session-Token' => $session['token']];
    $this->putJson($endpoint.'/offer', ['description' => ['type' => 'offer', 'sdp' => 'v=0']], $headers)->assertNoContent();
    $this->putJson($endpoint.'/offer', ['description' => ['type' => 'offer', 'sdp' => 'v=1']], $headers)->assertConflict();
    $this->putJson($endpoint.'/offer', ['description' => ['type' => 'offer', 'sdp' => str_repeat('a', 70_000)]], $headers)->assertUnprocessable();
    $this->patchJson($endpoint, ['status' => 'completed', 'progress' => 100], $headers)->assertNoContent();
    $this->patchJson($endpoint, ['status' => 'completed', 'progress' => 100], $headers)->assertNoContent();
    $this->patchJson($endpoint, ['status' => 'failed', 'progress' => 100], $headers)->assertConflict();
});

test('signaling preserves SDP line endings byte for byte in both directions', function (): void {
    $transfer = reserveWebRtc();
    publishWebRtc($transfer);
    $session = registerWebRtc($transfer);
    $endpoint = "/api/v1/transfers/{$transfer['id']}/webrtc/sessions";
    $sdp = "v=0\r\ns=-\r\na=max-message-size:262144\r\n";
    $sender = ['X-Filebeam-Upload-Token' => $transfer['upload_token']];
    $receiver = ['X-Filebeam-Session-Token' => $session['token']];
    $this->putJson("{$endpoint}/{$session['id']}/offer", ['description' => ['type' => 'offer', 'sdp' => $sdp]], $receiver)->assertNoContent();
    $this->getJson($endpoint, $sender)->assertOk()->assertJsonPath('data.sessions.0.offer.sdp', $sdp);
    $this->putJson("{$endpoint}/{$session['id']}/answer", ['description' => ['type' => 'answer', 'sdp' => $sdp]], $sender)->assertNoContent();
    $this->getJson("{$endpoint}/{$session['id']}", $receiver)->assertOk()->assertJsonPath('data.answer.sdp', $sdp);
});

test('burn notes have one durable claimant and consumption requires its session token', function (): void {
    $transfer = reserveWebRtc('note', true);
    publishWebRtc($transfer);
    $session = registerWebRtc($transfer);
    $this->postJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions", [], ['X-Filebeam-Join-Token' => $transfer['join_token']])->assertConflict();
    $this->postJson("/api/v1/transfers/{$transfer['id']}/consume", [], ['X-Filebeam-Read-Token' => $transfer['read_token'], 'X-Filebeam-Session-Token' => 'wrong'])->assertForbidden();
    $this->postJson("/api/v1/transfers/{$transfer['id']}/consume", [], ['X-Filebeam-Read-Token' => $transfer['read_token'], 'X-Filebeam-Session-Token' => $session['token']])->assertAccepted();
});

test('an expired claimed session cannot consume a burn note after a sender heartbeat refreshes cache TTL', function (): void {
    $transfer = reserveWebRtc('note', true);
    publishWebRtc($transfer);
    $session = registerWebRtc($transfer);
    Cache::put("filebeam:webrtc:{$transfer['id']}:sessions", [[
        'id' => $session['id'], 'token_hash' => hash('sha256', $session['token']), 'offer' => null, 'answer' => null,
        'status' => 'waiting', 'progress' => 0, 'expires_at' => now()->addSecond()->getTimestamp(),
    ]], now()->addMinute());
    $this->getJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions", ['X-Filebeam-Upload-Token' => $transfer['upload_token']])->assertOk();
    $this->travel(2)->seconds();
    $this->postJson("/api/v1/transfers/{$transfer['id']}/consume", [], ['X-Filebeam-Read-Token' => $transfer['read_token'], 'X-Filebeam-Session-Token' => $session['token']])->assertForbidden();
    expect(Transfer::query()->findOrFail($transfer['id'])->status)->toBe(TransferStatus::Live);
});

test('policy gates pending publication and future joins without interrupting live sender sessions', function (): void {
    $transfer = reserveWebRtc();
    config()->set('filebeam.instance_settings.environment.enabled_drivers', ['http']);
    $this->putJson("/api/v1/transfers/{$transfer['id']}/webrtc/publish", ['encrypted_manifest' => 'encrypted-manifest'], ['X-Filebeam-Upload-Token' => $transfer['upload_token']])->assertNotFound();
    config()->set('filebeam.instance_settings.environment.enabled_drivers', ['http', 'webrtc']);
    publishWebRtc($transfer);
    $session = registerWebRtc($transfer);
    config()->set('filebeam.instance_settings.environment.enabled_drivers', ['http']);
    $this->getJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions", ['X-Filebeam-Upload-Token' => $transfer['upload_token']])->assertOk();
    $this->getJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions/{$session['id']}", ['X-Filebeam-Session-Token' => $session['token']])->assertOk();
    $this->postJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions", [], ['X-Filebeam-Join-Token' => $transfer['join_token']])->assertNotFound();
});

test('sender expiry and end revoke admitted sessions', function (): void {
    $transfer = reserveWebRtc();
    publishWebRtc($transfer);
    $session = registerWebRtc($transfer);
    Cache::forget("filebeam:webrtc:{$transfer['id']}:sender");
    $this->getJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions/{$session['id']}", ['X-Filebeam-Session-Token' => $session['token']])->assertNotFound();
    $this->getJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions", ['X-Filebeam-Upload-Token' => $transfer['upload_token']])->assertOk();
    $this->postJson("/api/v1/transfers/{$transfer['id']}/webrtc/end", [], ['X-Filebeam-Upload-Token' => $transfer['upload_token']])->assertNoContent();
    $this->getJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions/{$session['id']}", ['X-Filebeam-Session-Token' => $session['token']])->assertNotFound();
});

test('session capacity is bounded and expired entries cannot be revived', function (): void {
    config()->set('filebeam.webrtc.session_limit', 1);
    $transfer = reserveWebRtc();
    publishWebRtc($transfer);
    $first = registerWebRtc($transfer);
    $this->postJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions", [], ['X-Filebeam-Join-Token' => $transfer['join_token']])->assertTooManyRequests();
    Cache::put("filebeam:webrtc:{$transfer['id']}:sessions", [[
        'id' => $first['id'], 'token_hash' => hash('sha256', $first['token']), 'offer' => null, 'answer' => null,
        'status' => 'waiting', 'progress' => 0, 'expires_at' => now()->subSecond()->getTimestamp(),
    ]], now()->addMinute());
    $replacement = registerWebRtc($transfer);
    $this->getJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions/{$first['id']}", ['X-Filebeam-Session-Token' => $first['token']])->assertNotFound();
    expect($replacement['id'])->not->toBe($first['id']);
});

test('webrtc nullable quotas are independent and disabled driver is rejected', function (): void {
    Plan::query()->where('slug', 'default')->sole()->update(['webrtc_maximum_transfer_bytes' => null, 'webrtc_maximum_file_count' => 1, 'webrtc_maximum_note_bytes' => 15]);
    reserveWebRtc();
    $this->postJson('/api/v1/transfers', ['kind' => 'note', 'driver' => 'webrtc', 'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'), 'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]]])->assertUnprocessable()->assertJsonValidationErrors('items');
    config()->set('filebeam.instance_settings.environment.enabled_drivers', ['http']);
    $this->postJson('/api/v1/transfers', ['kind' => 'files', 'driver' => 'webrtc', 'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'), 'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]]])->assertUnprocessable()->assertJsonValidationErrors('driver');
});

test('unlimited WebRTC still rejects ciphertext declarations inconsistent with the encrypted chunk protocol', function (): void {
    Plan::query()->where('slug', 'default')->sole()->update(['webrtc_maximum_transfer_bytes' => null]);
    $this->postJson('/api/v1/transfers', [
        'kind' => 'files', 'driver' => 'webrtc', 'protocol_version' => 1,
        'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 2]],
    ])->assertUnprocessable()->assertJsonValidationErrors('items.0.ciphertext_bytes');
});

test('TURN credentials are authorized and never disclose the configured secret', function (): void {
    config()->set('filebeam.webrtc.turn_urls', ['turn:relay.example.test']);
    config()->set('filebeam.webrtc.turn_secret', 'secret-value');
    $transfer = reserveWebRtc();
    publishWebRtc($transfer);
    $session = registerWebRtc($transfer);
    $this->getJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions/{$session['id']}", ['X-Filebeam-Session-Token' => $session['token']])->assertJsonMissing(['secret-value']);
    $this->getJson("/api/v1/transfers/{$transfer['id']}/webrtc/sessions", ['X-Filebeam-Upload-Token' => $transfer['upload_token']])->assertJsonMissing(['secret-value'])->assertJsonFragment(['urls' => ['turn:relay.example.test']]);
});

test('rotating arbitrary signaling tokens cannot bypass the overall IP rate budget', function (): void {
    $first = Request::create('/api/v1/transfers/example/webrtc/sessions/example');
    $first->headers->set('X-Filebeam-Session-Token', 'first');
    $second = clone $first;
    $second->headers->set('X-Filebeam-Session-Token', 'second');
    $limiter = RateLimiter::limiter('webrtc-session');
    $firstLimits = $limiter($first);
    $secondLimits = $limiter($second);
    expect($firstLimits)->toHaveCount(2)
        ->and($firstLimits[0]->key)->toBe($secondLimits[0]->key)
        ->and($firstLimits[0]->maxAttempts)->toBe(1200)
        ->and($firstLimits[1]->key)->not->toBe($secondLimits[1]->key);
});
