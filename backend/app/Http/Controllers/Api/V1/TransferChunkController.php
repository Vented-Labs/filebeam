<?php

declare(strict_types=1);

namespace App\Http\Controllers\Api\V1;

use App\Enums\TransferDelivery;
use App\Enums\TransferStatus;
use App\Http\Controllers\Controller;
use App\Models\Filestore;
use App\Models\Transfer;
use App\Models\TransferChunk;
use App\Models\TransferChunkUpload;
use App\Models\TransferItem;
use App\Support\Capability;
use App\Support\ChunkReader;
use App\Support\FilestoreRegistry;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Str;
use RuntimeException;
use Symfony\Component\HttpFoundation\StreamedResponse;
use Throwable;

class TransferChunkController extends Controller
{
    public function __construct(private FilestoreRegistry $stores) {}

    /**
     * @throws Throwable
     */
    public function store(Request $request, Transfer $transfer, TransferItem $item, int $position): JsonResponse
    {
        $this->authorizeUpload($transfer, $request->header('X-Filebeam-Upload-Token'));
        abort_unless($transfer->status === TransferStatus::Pending && $transfer->expires_at->isFuture(), 404);
        abort_unless($position >= 0 && $position < $item->chunk_count, 404);
        $contentLength = $request->header('Content-Length');
        abort_if(
            is_string($contentLength) && ctype_digit($contentLength)
                && (int) $contentLength !== $this->expectedCiphertextBytes($item, $transfer, $position),
            422,
            'Invalid ciphertext chunk size.',
        );
        [$temporaryStream, $ciphertextBytes, $checksum] = self::spoolCiphertext($request, $transfer->chunk_bytes + 16);

        try {
            $attempts = DB::transaction(function () use ($request, $transfer, $item, $position, $ciphertextBytes, $checksum): array {
                $lockedTransfer = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
                $this->authorizeUpload($lockedTransfer, $request->header('X-Filebeam-Upload-Token'));
                abort_unless($lockedTransfer->status === TransferStatus::Pending && $lockedTransfer->expires_at->isFuture(), 404);
                $lockedItem = TransferItem::query()->lockForUpdate()->findOrFail($item->id);
                abort_unless($position < $lockedItem->chunk_count, 404);
                abort_unless($ciphertextBytes === $this->expectedCiphertextBytes($lockedItem, $lockedTransfer, $position), 422, 'Invalid ciphertext chunk size.');

                $chunk = $lockedItem->chunks()->atPosition($position)->first();
                if ($chunk !== null) {
                    abort_unless(hash_equals($chunk->checksum, $checksum), 409, 'Chunk position already contains different ciphertext.');
                }

                $storedIds = $chunk?->locations()->pluck('filestore_id')->all() ?? [];
                if ($lockedTransfer->placement_mode === 'distribute' && $storedIds !== []) {
                    return [];
                }

                $selectedIds = $lockedTransfer->filestore_ids ?? [];
                $missingIds = array_values(array_diff($selectedIds, $storedIds));
                $stores = Filestore::query()->whereKey($missingIds)->orderBy('id')->lockForUpdate()->get();
                abort_unless($selectedIds !== [] && $stores->count() === count($missingIds), 503, 'The selected filestores are unavailable.');

                if ($lockedTransfer->placement_mode === 'distribute') {
                    // Concurrent attempts use separate objects, but retain the first reserved destination.
                    $reservedId = TransferChunkUpload::query()->where('transfer_item_id', $item->id)
                        ->atPosition($position)->orderBy('id')->value('filestore_id');
                    $eligible = $stores->filter(fn (Filestore $store): bool => $this->stores->placementEnabled($store));
                    $store = $reservedId !== null ? $stores->firstWhere('id', $reservedId) : ($eligible->isEmpty() ? null : $eligible->random());
                    abort_unless($store !== null && $this->stores->placementEnabled($store), 503, 'No selected filestore accepts new placement.');
                    $stores = $stores->where('id', $store->id);
                }

                $attempts = [];
                foreach ($stores as $store) {
                    abort_unless($this->stores->placementEnabled($store), 503, 'A required filestore is not accepting new placement.');
                    $attemptId = (string) Str::ulid();
                    $attempts[] = TransferChunkUpload::query()->create([
                        'id' => $attemptId,
                        'transfer_id' => $lockedTransfer->id,
                        'transfer_item_id' => $lockedItem->id,
                        'position' => $position,
                        'filestore_id' => $store->id,
                        'storage_path' => 'transfers/'.$lockedTransfer->id.'/attempts/'.$attemptId.'.bin',
                        'ciphertext_bytes' => $ciphertextBytes,
                        'checksum' => $checksum,
                        // Cover all sequential replica writes, each bounded by the provider timeout.
                        'valid_until' => now()->addSeconds(
                            (int) config('filebeam.transfers.upload_attempt_lease_seconds') * $stores->count()
                            + (int) config('filebeam.transfers.upload_attempt_cleanup_grace_seconds'),
                        ),
                    ]);
                }

                return $attempts;
            });

            foreach ($attempts as $attempt) {
                rewind($temporaryStream);
                try {
                    $written = $this->stores->disk($attempt->filestore)->put($attempt->storage_path, $temporaryStream);
                } catch (Throwable) {
                    abort(503, 'Unable to store a required ciphertext copy. Retry the chunk upload.');
                }
                abort_unless($written === true, 503, 'Unable to store a required ciphertext copy. Retry the chunk upload.');

                try {
                    $result = DB::transaction(function () use ($transfer, $item, $position, $attempt, $checksum): string {
                        $lockedTransfer = Transfer::query()->lockForUpdate()->find($transfer->id);
                        $lockedItem = TransferItem::query()->lockForUpdate()->find($item->id);
                        $lockedAttempt = TransferChunkUpload::query()->lockForUpdate()->find($attempt->id);

                        if ($lockedAttempt === null || $lockedTransfer === null || $lockedItem === null
                            || ! $lockedAttempt->valid_until->isFuture()
                            || $lockedTransfer->status !== TransferStatus::Pending || ! $lockedTransfer->expires_at->isFuture()) {
                            return 'unavailable';
                        }

                        if ($lockedAttempt->is_reaping) {
                            return 'reaping';
                        }

                        $chunk = $lockedItem->chunks()->atPosition($position)->first();
                        if ($chunk !== null && ! hash_equals($chunk->checksum, $checksum)) {
                            return 'conflict';
                        }

                        if ($chunk !== null && ($lockedTransfer->placement_mode === 'distribute'
                            ? $chunk->locations()->exists()
                            : $chunk->locations()->where('filestore_id', $lockedAttempt->filestore_id)->exists())) {
                            return 'duplicate';
                        }

                        if ($chunk === null) {
                            $chunk = $lockedItem->chunks()->create([
                                'position' => $position,
                                'ciphertext_bytes' => $lockedAttempt->ciphertext_bytes,
                                'checksum' => $checksum,
                            ]);
                            $lockedItem->increment('ciphertext_bytes', $lockedAttempt->ciphertext_bytes);
                            if ($lockedTransfer->isPublishedTurbo()) {
                                $maximumExpiry = $lockedTransfer->created_at->addHours((int) config('filebeam.transfers.pending_max_lifetime_hours'));
                                $extendedExpiry = now()->addHours((int) config('filebeam.transfers.incomplete_expiry_hours'));
                                $lockedTransfer->update(['expires_at' => $extendedExpiry->lessThan($maximumExpiry) ? $extendedExpiry : $maximumExpiry]);
                            }
                        }
                        $chunk->locations()->create([
                            'filestore_id' => $lockedAttempt->filestore_id,
                            'storage_path' => $lockedAttempt->storage_path,
                            'ciphertext_bytes' => $lockedAttempt->ciphertext_bytes,
                        ]);
                        $lockedAttempt->delete();

                        return 'accepted';
                    });
                } catch (Throwable $exception) {
                    $this->removeAttempt($attempt);

                    throw $exception;
                }

                if ($result === 'accepted') {
                    continue;
                }

                if ($result === 'reaping') {
                    abort(503, 'Chunk upload cleanup is in progress.');
                }
                $this->removeAttempt($attempt);
                if ($result === 'unavailable') {
                    abort(404);
                }
                if ($result === 'conflict') {
                    abort(409, 'Chunk position already contains different ciphertext.');
                }
            }

            return response()->json(['checksum' => $checksum], 201);
        } finally {
            fclose($temporaryStream);
        }
    }

    private function authorizeUpload(Transfer $transfer, ?string $token): void
    {
        abort_unless(Capability::matches($transfer->upload_token_hash, $token), 403);
    }

    private function expectedCiphertextBytes(TransferItem $item, Transfer $transfer, int $position): int
    {
        return min($transfer->chunk_bytes + 16, $item->declared_ciphertext_bytes - ($position * ($transfer->chunk_bytes + 16)));
    }

    /** @return array{0: resource, 1: int, 2: string}
     * @throws Throwable
     */
    private static function spoolCiphertext(Request $request, int $maximumChunkBytes): array
    {
        $contentLength = $request->header('Content-Length');
        abort_if(is_string($contentLength) && (! ctype_digit($contentLength) || (int) $contentLength > $maximumChunkBytes), 422, 'Invalid ciphertext chunk size.');
        $temporaryStream = tmpfile();
        throw_unless(is_resource($temporaryStream), new RuntimeException('Unable to create ciphertext spool.'));
        $input = $request->getContent(asResource: true);
        $hash = hash_init('sha256');
        $bytes = 0;

        try {
            while (! feof($input)) {
                $remaining = $maximumChunkBytes + 1 - $bytes;
                abort_if($remaining < 1, 422, 'Invalid ciphertext chunk size.');
                $chunk = fread($input, min(1_048_576, $remaining));
                if ($chunk === false) {
                    throw new RuntimeException('Unable to read ciphertext chunk.');
                }
                $bytes += strlen($chunk);
                abort_if($bytes > $maximumChunkBytes, 422, 'Invalid ciphertext chunk size.');
                hash_update($hash, $chunk);
                $offset = 0;
                while ($offset < strlen($chunk)) {
                    $written = fwrite($temporaryStream, substr($chunk, $offset));
                    if ($written === false || $written === 0) {
                        throw new RuntimeException('Unable to spool ciphertext chunk.');
                    }
                    $offset += $written;
                }
            }
        } catch (Throwable $exception) {
            fclose($temporaryStream);

            throw $exception;
        } finally {
            // Symfony creates an input resource for string request content too.
            fclose($input);
        }

        return [$temporaryStream, $bytes, hash_final($hash)];
    }

    private function removeAttempt(TransferChunkUpload $attempt): void
    {
        if ($this->stores->disk($attempt->filestore)->delete($attempt->storage_path) !== true) {
            throw new RuntimeException('Unable to remove ciphertext upload attempt.');
        }

        TransferChunkUpload::query()->whereKey($attempt->id)->delete();
    }

    public function show(Transfer $transfer, TransferItem $item, int $position, ChunkReader $reader): StreamedResponse|JsonResponse
    {
        abort_unless($transfer->delivery === TransferDelivery::Link && $transfer->expires_at->isFuture(), 404);
        $isAvailable = $transfer->status === TransferStatus::Available;
        $isPublishedPendingTurbo = $transfer->status === TransferStatus::Pending && $transfer->isPublishedTurbo();
        abort_unless($isAvailable || $isPublishedPendingTurbo, 404);
        abort_unless($position >= 0 && $position < $item->chunk_count, 404);
        $chunk = $item->chunks()->atPosition($position)->first();
        if ($chunk === null && $isPublishedPendingTurbo) {
            return response()->json(['status' => 'pending'], 202, ['Cache-Control' => 'no-store', 'Retry-After' => '2']);
        }
        abort_unless($chunk instanceof TransferChunk, 404);
        $stream = $reader->read($chunk);

        return response()->stream(function () use ($stream): void {
            try {
                fpassthru($stream);
            } finally {
                fclose($stream);
            }
        }, 200, [
            'Cache-Control' => 'private, no-store',
            'Content-Length' => (string) $chunk->ciphertext_bytes,
            'Content-Type' => 'application/octet-stream',
            'X-Content-Type-Options' => 'nosniff',
        ]);
    }
}
