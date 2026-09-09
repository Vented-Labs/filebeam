<?php

declare(strict_types=1);

namespace App\Http\Controllers\Api\V1;

use App\Enums\TransferDriver;
use App\Enums\TransferStatus;
use App\Http\Controllers\Controller;
use App\Http\Requests\PublishTransferDescriptorRequest;
use App\Models\Transfer;
use App\Models\TransferItem;
use App\Support\Capability;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;
use Throwable;

class TurboTransferController extends Controller
{
    /**
     * @throws Throwable
     */
    public function descriptor(PublishTransferDescriptorRequest $request, Transfer $transfer): JsonResponse
    {
        /** @var array{encrypted_descriptor: string} $validated */
        $validated = $request->validated();
        DB::transaction(function () use ($request, $transfer, $validated): void {
            $lockedTransfer = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
            $this->authorizeUpload($lockedTransfer, $request->header('X-Filebeam-Upload-Token'));
            abort_unless($this->isPendingTurbo($lockedTransfer), 404);

            if ($lockedTransfer->encrypted_descriptor !== null) {
                abort_unless(hash_equals($lockedTransfer->encrypted_descriptor, $validated['encrypted_descriptor']), 409);

                return;
            }

            $lockedTransfer->update(['encrypted_descriptor' => $validated['encrypted_descriptor']]);
        });
        try {
            $this->pulse($transfer->refresh());
        } catch (Throwable) {
            // Descriptor publication is durable; monitoring can recover on the next heartbeat.
        }

        return response()->json(status: 204);
    }

    private function authorizeUpload(Transfer $transfer, ?string $token): void
    {
        abort_unless(Capability::matches($transfer->upload_token_hash, $token), 403);
    }

    private function isPendingTurbo(Transfer $transfer): bool
    {
        return $transfer->driver === TransferDriver::Http
            && $transfer->kind->value === 'files'
            && $transfer->delivery->value === 'link'
            && $transfer->protocol_version === 1
            && $transfer->status === TransferStatus::Pending
            && $transfer->expires_at->isFuture();
    }

    private function pulse(Transfer $transfer): void
    {
        try {
            Cache::put($this->pulseKey($transfer), true, now()->addSeconds(31));
        } catch (Throwable) {
            abort(503, 'Upload monitoring is temporarily unavailable.');
        }
    }

    private function pulseKey(Transfer $transfer): string
    {
        return "filebeam:turbo:{$transfer->id}:pulse";
    }

    public function progress(Transfer $transfer): JsonResponse
    {
        abort_unless($transfer->isPublishedTurbo() && $transfer->expires_at->isFuture(), 404);
        $transfer->load('items');
        $committedBytes = $transfer->items->sum('ciphertext_bytes');
        $complete = $transfer->status === TransferStatus::Available;
        $progress = $complete ? 100 : min(99, (int) floor(($committedBytes * 100) / max(1, $transfer->declared_ciphertext_bytes)));

        try {
            $uploaderStatus = $complete ? 'completed' : (Cache::get($this->pulseKey($transfer)) ? 'uploading' : 'stalled');
        } catch (Throwable) {
            $uploaderStatus = 'unavailable';
        }

        return response()->json(['data' => [
            'status' => $complete ? 'available' : 'pending',
            'progress' => $progress,
            'expires_at' => $transfer->expires_at->toIso8601String(),
            'uploader_status' => $uploaderStatus,
            'items' => $transfer->items->map(fn (TransferItem $item): array => [
                'id' => $item->id,
                'ready_chunks' => $this->readyChunks($item),
                'uploaded_chunks' => $item->chunks()->count(),
            ])->all(),
        ]]);
    }

    private function readyChunks(TransferItem $item): int
    {
        $positions = $item->chunks()->orderBy('position')->pluck('position');
        $ready = 0;
        foreach ($positions as $position) {
            if ($position !== $ready) {
                break;
            }
            $ready++;
        }

        return $ready;
    }

    public function heartbeat(Request $request, Transfer $transfer): JsonResponse
    {
        $this->authorizeUpload($transfer, $request->header('X-Filebeam-Upload-Token'));
        abort_unless($this->isPendingTurbo($transfer) && $transfer->isPublishedTurbo(), 404);
        $this->pulse($transfer);

        return response()->json(status: 204);
    }

    public function monitor(Request $request, Transfer $transfer): JsonResponse
    {
        abort_unless($transfer->isPublishedTurbo() && $transfer->expires_at->isFuture(), 404);
        abort_unless(Capability::matches($transfer->monitor_token_hash, $request->header('X-Filebeam-Monitor-Token')), 403);

        try {
            /** @var list<array{id: string, number: int, progress: int|float, status: string, selection_count: int, all_files: bool, heartbeat_at: int, expires_at: int}> $cachedSessions */
            $cachedSessions = Cache::get($this->sessionsKey($transfer), []);
            $sessions = collect($cachedSessions)
                ->filter(fn (array $session): bool => $session['expires_at'] > now()->getTimestamp())
                ->map(fn (array $session): array => [
                    'id' => $session['id'],
                    'number' => $session['number'],
                    'progress' => $session['progress'],
                    'status' => $this->monitorStatus($session),
                    'selection_count' => $session['selection_count'],
                    'all_files' => $session['all_files'],
                ])->values()->all();
        } catch (Throwable) {
            abort(503, 'Download monitoring is temporarily unavailable.');
        }

        return response()->json(['data' => ['sessions' => $sessions]]);
    }

    private function sessionsKey(Transfer $transfer): string
    {
        return "filebeam:turbo:{$transfer->id}:sessions";
    }

    /** @param array{id: string, number: int, progress: int|float, status: string, selection_count: int, all_files: bool, heartbeat_at: int, expires_at: int} $session */
    private function monitorStatus(array $session): string
    {
        if (in_array($session['status'], ['completed', 'cancelled', 'error'], true)) {
            return $session['status'];
        }

        return $session['heartbeat_at'] > now()->subSeconds(30)->getTimestamp() ? $session['status'] : 'stale';
    }
}
