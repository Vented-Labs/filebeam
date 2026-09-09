<?php

declare(strict_types=1);

namespace App\Http\Controllers\Api\V1;

use App\Enums\TransferDriver;
use App\Enums\TransferKind;
use App\Enums\TransferStatus;
use App\Http\Controllers\Controller;
use App\Http\Requests\PublishWebRtcTransferRequest;
use App\Http\Resources\TransferResource;
use App\Models\Transfer;
use App\Support\Capability;
use App\Support\TransportPolicy;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Str;
use Symfony\Component\HttpFoundation\Response;

class WebRtcTransferController extends Controller
{
    /** @var list<string> */
    private const TERMINAL = ['completed', 'cancelled', 'failed'];

    public function publish(PublishWebRtcTransferRequest $request, Transfer $transfer): JsonResponse
    {
        $manifest = $request->validated('encrypted_manifest');
        DB::transaction(function () use ($request, $transfer, $manifest): void {
            $locked = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
            $this->upload($locked, $request);
            $this->assertPendingOrLive($locked);
            if ($locked->status === TransferStatus::Live) {
                abort_unless(hash_equals($locked->encrypted_manifest ?? '', $manifest), 409);

                return;
            }
            abort_unless(app(TransportPolicy::class)->allows(TransferDriver::WebRtc), 404);
            $maximum = $locked->created_at->addHours((int) config('filebeam.webrtc.live_max_hours', 24));
            $retention = now()->addHours($locked->retention_hours);
            $locked->update(['encrypted_manifest' => $manifest, 'status' => TransferStatus::Live, 'expires_at' => $retention->lessThan($maximum) ? $retention : $maximum]);
        });
        $this->pulseSender($transfer);

        return (new TransferResource($transfer->refresh()->load('items')))->response();
    }

    public function sessions(Request $request, Transfer $transfer): JsonResponse
    {
        $this->upload($transfer, $request);
        $this->assertLive($transfer);
        /** @var list<array{id: string, offer: array{type: string, sdp: string}|null, status: string, progress: int}> $sessions */
        $sessions = $this->withSessions($transfer, function (array $sessions) use ($transfer): array {
            $this->prune($sessions);
            $this->assertCurrentLive($transfer);
            $this->pulseSender($transfer);

            return [$sessions, array_map(fn (array $session): array => ['id' => $session['id'], 'offer' => $session['offer'], 'status' => $session['status'], 'progress' => $session['progress']], $sessions)];
        });

        return response()->json(['data' => ['sessions' => $sessions, 'ice_servers' => $this->iceServers(), 'expires_at' => $transfer->expires_at->toIso8601String()]], 200, ['Cache-Control' => 'no-store']);
    }

    public function register(Request $request, Transfer $transfer): JsonResponse
    {
        $this->assertLive($transfer);
        abort_unless(app(TransportPolicy::class)->allows(TransferDriver::WebRtc), 404);
        abort_unless(Capability::matches($transfer->join_token_hash ?? '', $request->header('X-Filebeam-Join-Token')), 403);
        abort_unless($this->senderIsLive($transfer), 404);
        $id = (string) Str::ulid();
        $token = Str::random(64);

        $this->withSessions($transfer, function (array $sessions) use ($transfer, $id, $token): array {
            $this->prune($sessions);
            $this->assertCurrentLive($transfer);
            abort_unless($this->senderIsLive($transfer), 404);
            // Terminal reports are retained briefly for idempotency but never consume admission capacity.
            $sessions = array_values(array_filter($sessions, fn (array $session): bool => ! in_array($session['status'], self::TERMINAL, true)));
            abort_if(count($sessions) >= $this->limit(), 429, 'Session capacity reached.');
            DB::transaction(function () use ($transfer, $id): void {
                $locked = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
                $this->assertLive($locked);
                if ($locked->kind === TransferKind::Note && $locked->burn_on_read) {
                    // A reservation is deliberately never reassigned: cache loss cannot prove that no payload was delivered.
                    abort_unless($locked->webrtc_claimed_session_id === null, 409);
                    $locked->update(['webrtc_claimed_session_id' => $id]);
                }
            });
            $sessions[] = $this->newSession($id, $token);

            return [$sessions, null];
        });

        return response()->json(['data' => ['id' => $id, 'token' => $token, 'ice_servers' => $this->iceServers(), 'expires_at' => $transfer->expires_at->toIso8601String()]], 201, ['Cache-Control' => 'no-store']);
    }

    public function offer(Request $request, Transfer $transfer, string $session): Response
    {
        $description = $this->description($request, 'offer');
        $this->mutateReceiver($transfer, $session, $request, function (array &$entry) use ($description): void {
            $this->notTerminal($entry);
            abort_unless($entry['offer'] === null || $entry['offer'] === $description, 409);
            $entry['offer'] = $description;
        });

        return response()->noContent();
    }

    public function answer(Request $request, Transfer $transfer, string $session): Response
    {
        $this->upload($transfer, $request);
        $this->assertLive($transfer);
        abort_unless($this->senderIsLive($transfer), 404);
        $description = $this->description($request, 'answer');
        $this->withSessions($transfer, function (array $sessions) use ($transfer, $session, $description): array {
            $this->prune($sessions);
            $this->assertCurrentLive($transfer);
            abort_unless($this->senderIsLive($transfer), 404);
            foreach ($sessions as &$entry) {
                if ($entry['id'] !== $session) {
                    continue;
                }
                $this->notTerminal($entry);
                abort_unless($entry['offer'] !== null, 409, 'An offer is required before an answer.');
                abort_unless($entry['answer'] === null || $entry['answer'] === $description, 409);
                $entry['answer'] = $description;

                return [$sessions, null];
            }
            abort(404);
        });

        return response()->noContent();
    }

    public function show(Request $request, Transfer $transfer, string $session): JsonResponse
    {
        $this->assertLive($transfer);
        abort_unless($this->senderIsLive($transfer), 404);
        /** @var array{answer: array{type: string, sdp: string}|null, status: string} $entry */
        $entry = $this->mutateReceiver($transfer, $session, $request, static function (array &$entry): void {});

        return response()->json(['data' => ['answer' => $entry['answer'], 'status' => $entry['status'], 'expires_at' => $transfer->expires_at->toIso8601String()]], 200, ['Cache-Control' => 'no-store']);
    }

    public function update(Request $request, Transfer $transfer, string $session): Response
    {
        /** @var array{status: string, progress: int} $validated */
        $validated = $request->validate(['status' => ['required', 'in:active,completed,cancelled,failed'], 'progress' => ['required', 'integer', 'min:0', 'max:100']]);
        $this->mutateReceiver($transfer, $session, $request, function (array &$entry) use ($validated): void {
            if (in_array($entry['status'], self::TERMINAL, true)) {
                abort_unless($entry['status'] === $validated['status'] && $entry['progress'] === $validated['progress'], 409);

                return;
            }
            abort_unless($validated['progress'] >= $entry['progress'], 409);
            $entry['status'] = $validated['status'];
            $entry['progress'] = $validated['progress'];
            if (in_array($entry['status'], self::TERMINAL, true)) {
                $entry['expires_at'] = now()->addSeconds($this->idle())->getTimestamp();
            }
        });

        return response()->noContent();
    }

    public function end(Request $request, Transfer $transfer): Response
    {
        DB::transaction(function () use ($request, $transfer): void {
            $locked = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
            $this->upload($locked, $request);
            abort_unless($locked->driver === TransferDriver::WebRtc && $locked->expires_at->isFuture(), 404);
            abort_unless(in_array($locked->status, [TransferStatus::Live, TransferStatus::Ended], true), 404);
            if ($locked->status === TransferStatus::Live) {
                $locked->update(['status' => TransferStatus::Ended]);
            }
        });
        $this->revoke($transfer);

        return response()->noContent();
    }

    /** @return array{type: string, sdp: string} */
    private function description(Request $request, string $type): array
    {
        $contentLength = $request->header('Content-Length');
        $maximum = (int) config('filebeam.webrtc.max_sdp_bytes', 65_536) + 512;
        abort_if(is_string($contentLength) && (! ctype_digit($contentLength) || (int) $contentLength > $maximum), 422, 'Invalid SDP.');
        $description = $request->validate(['description.type' => ['required', 'in:'.$type], 'description.sdp' => ['required', 'string']])['description'];
        abort_unless(is_array($description) && is_string($description['sdp']) && strlen($description['sdp']) <= (int) config('filebeam.webrtc.max_sdp_bytes', 65536) && preg_match('/\A[\x09\x0A\x0D\x20-\x7E]*\z/', $description['sdp']) === 1, 422, 'Invalid SDP.');

        return ['type' => $type, 'sdp' => $description['sdp']];
    }

    /** @param callable(array<string, mixed>&): void $mutate */
    /** @return array<string, mixed> */
    private function mutateReceiver(Transfer $transfer, string $id, Request $request, callable $mutate): array
    {
        $this->assertLive($transfer);

        return $this->withSessions($transfer, function (array $sessions) use ($transfer, $id, $request, $mutate): array {
            $this->prune($sessions);
            $this->assertCurrentLive($transfer);
            abort_unless($this->senderIsLive($transfer), 404);
            foreach ($sessions as &$entry) {
                if ($entry['id'] !== $id) {
                    continue;
                }
                $this->token($entry, $request);
                $mutate($entry);
                if (! in_array($entry['status'], self::TERMINAL, true)) {
                    $entry['expires_at'] = now()->addSeconds($this->idle())->getTimestamp();
                }

                return [$sessions, $entry];
            }
            abort(404);
        });
    }

    /** @param array<string, mixed> $entry */
    private function notTerminal(array $entry): void
    {
        abort_unless(! in_array($entry['status'], self::TERMINAL, true), 409);
    }

    /** @param array<string, mixed> $entry */
    private function token(array $entry, Request $request): void
    {
        abort_unless(Capability::matches((string) $entry['token_hash'], $request->header('X-Filebeam-Session-Token')), 403);
    }

    private function upload(Transfer $transfer, Request $request): void
    {
        abort_unless(Capability::matches($transfer->upload_token_hash, $request->header('X-Filebeam-Upload-Token')), 403);
    }

    private function assertPendingOrLive(Transfer $transfer): void
    {
        abort_unless($transfer->driver === TransferDriver::WebRtc && $transfer->delivery->value === 'link' && $transfer->expires_at->isFuture() && in_array($transfer->status, [TransferStatus::Pending, TransferStatus::Live], true), 404);
    }

    private function assertLive(Transfer $transfer): void
    {
        abort_unless($transfer->driver === TransferDriver::WebRtc && $transfer->delivery->value === 'link' && $transfer->status === TransferStatus::Live && $transfer->expires_at->isFuture(), 404);
    }

    private function assertCurrentLive(Transfer $transfer): void
    {
        $current = Transfer::query()->find($transfer->id);
        abort_unless($current instanceof Transfer, 404);
        $this->assertLive($current);
    }

    private function idle(): int
    {
        return max(10, (int) config('filebeam.webrtc.session_idle_seconds', 120));
    }

    private function limit(): int
    {
        return max(1, (int) config('filebeam.webrtc.session_limit', 8));
    }

    private function key(Transfer $transfer): string
    {
        return "filebeam:webrtc:{$transfer->id}:sessions";
    }

    private function senderKey(Transfer $transfer): string
    {
        return "filebeam:webrtc:{$transfer->id}:sender";
    }

    private function pulseSender(Transfer $transfer): void
    {
        Cache::put($this->senderKey($transfer), now()->getTimestamp(), now()->addSeconds($this->idle()));
    }

    private function senderIsLive(Transfer $transfer): bool
    {
        return is_int(Cache::get($this->senderKey($transfer)));
    }

    private function revoke(Transfer $transfer): void
    {
        Cache::forget($this->key($transfer));
        Cache::forget($this->senderKey($transfer));
    }

    /** @param array<array<string, mixed>> $sessions */
    private function prune(array &$sessions): void
    {
        $now = now()->getTimestamp();
        $sessions = array_values(array_filter($sessions, static fn (array $session): bool => $session['expires_at'] > $now));
    }

    /** @param callable(array<array<string, mixed>>): array{0: array<array<string, mixed>>, 1: mixed} $callback */
    private function withSessions(Transfer $transfer, callable $callback): mixed
    {
        return Cache::lock("filebeam:webrtc:{$transfer->id}:lock", 5)->block(3, function () use ($transfer, $callback): mixed {
            $cached = Cache::get($this->key($transfer), []);
            /** @var list<array<string, mixed>> $sessions */
            $sessions = is_array($cached) ? $cached : [];
            [$sessions, $result] = $callback($sessions);
            Cache::put($this->key($transfer), $sessions, now()->addSeconds($this->idle()));

            return $result;
        });
    }

    /** @return array<string, mixed> */
    private function newSession(string $id, string $token): array
    {
        return ['id' => $id, 'token_hash' => hash('sha256', $token), 'offer' => null, 'answer' => null, 'status' => 'waiting', 'progress' => 0, 'expires_at' => now()->addSeconds($this->idle())->getTimestamp()];
    }

    /** @return list<array<string, mixed>> */
    private function iceServers(): array
    {
        $servers = config('filebeam.webrtc.ice_servers', []);
        $servers = is_array($servers) ? array_values($servers) : [];
        $urls = config('filebeam.webrtc.turn_urls', []);
        $secret = config('filebeam.webrtc.turn_secret');
        if (! is_array($urls) || $urls === [] || ! is_string($secret) || $secret === '') {
            return $servers;
        }
        $username = (string) now()->addSeconds((int) config('filebeam.webrtc.turn_ttl_seconds', 3600))->getTimestamp().':'.Str::random(16);
        $servers[] = ['urls' => array_values($urls), 'username' => $username, 'credential' => base64_encode(hash_hmac('sha1', $username, $secret, true))];

        return $servers;
    }
}
