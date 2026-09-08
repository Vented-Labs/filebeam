<?php

declare(strict_types=1);

namespace App\Http\Controllers\Api\V1;

use App\Enums\TransferStatus;
use App\Http\Controllers\Controller;
use App\Models\Transfer;
use App\Models\TransferChunkStage;
use App\Models\TransferItem;
use App\Support\Capability;
use App\Support\ChunkStaging;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use RuntimeException;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpKernel\Exception\HttpException;
use Throwable;

class TransferChunkStageController extends Controller
{
    public function __construct(private ChunkStaging $staging, private TransferChunkController $chunks) {}

    /** @throws Throwable */
    public function begin(Request $request, Transfer $transfer, TransferItem $item, int $position, string $upload): JsonResponse
    {
        $this->authorize($transfer, $request);
        abort_unless($this->validUuid($upload), 422, 'Invalid upload identifier.');
        $data = $request->validate(['ciphertext_bytes' => ['required', 'integer', 'min:1'], 'checksum' => ['required', 'string', 'regex:/^[a-f0-9]{64}$/']]);
        $created = null;
        try {
            $stage = $this->staging->capacityLock(function () use ($transfer, $item, $position, $upload, $data, &$created): TransferChunkStage {
                return DB::transaction(function () use ($transfer, $item, $position, $upload, $data, &$created): TransferChunkStage {
                    $lockedTransfer = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
                    $this->active($lockedTransfer);
                    $lockedItem = TransferItem::query()->lockForUpdate()->findOrFail($item->id);
                    abort_unless($position >= 0 && $position < $lockedItem->chunk_count, 404);
                    $expected = $this->expectedBytes($lockedTransfer, $lockedItem, $position);
                    abort_unless($data['ciphertext_bytes'] === $expected, 422, 'Invalid ciphertext chunk size.');
                    $chunk = $lockedItem->chunks()->atPosition($position)->first();
                    if ($chunk !== null && $this->published($lockedTransfer, $chunk->locations()->pluck('filestore_id')->all())) {
                        abort_unless(hash_equals($chunk->checksum, $data['checksum']), 409, 'Chunk position already contains different ciphertext.');

                        return new TransferChunkStage(['id' => $upload, 'ciphertext_bytes' => $expected, 'checksum' => $data['checksum'], 'offset' => $expected, 'state' => 'complete']);
                    }
                    $existing = TransferChunkStage::query()->lockForUpdate()->find($upload);
                    if ($existing !== null) {
                        abort_unless($existing->transfer_id === $lockedTransfer->id && $existing->transfer_item_id === $lockedItem->id
                            && $existing->position === $position && $existing->ciphertext_bytes === $expected && hash_equals($existing->checksum, $data['checksum']), 409, 'Upload identifier is already in use.');
                        abort_unless($existing->expires_at->isFuture(), 410, 'Upload staging has expired.');
                        if ($existing->state !== 'complete') {
                            $this->assertPrefix($existing);
                        }

                        return $existing;
                    }
                    $reserved = $expected * 2;
                    // Count every row, including expired and completed rows whose cleanup may have failed.
                    $reservations = TransferChunkStage::query()->whereNull('released_at');
                    $global = (int) (clone $reservations)->sum(DB::raw('ciphertext_bytes * 2'));
                    $perTransfer = (int) (clone $reservations)->where('transfer_id', $lockedTransfer->id)->sum(DB::raw('ciphertext_bytes * 2'));
                    $globalRows = (clone $reservations)->count();
                    $transferRows = (clone $reservations)->where('transfer_id', $lockedTransfer->id)->count();
                    if (($global + $reserved) > (int) config('filebeam.staging.global_bytes') || ($perTransfer + $reserved) > (int) config('filebeam.staging.transfer_bytes')
                        || $globalRows >= (int) config('filebeam.staging.global_max_rows') || $transferRows >= (int) config('filebeam.staging.transfer_max_rows')) {
                        throw new HttpException(503, 'Staging capacity is unavailable.', null, ['Retry-After' => '2']);
                    }

                    $stage = TransferChunkStage::query()->create([
                        'id' => $upload, 'transfer_id' => $lockedTransfer->id, 'transfer_item_id' => $lockedItem->id, 'position' => $position,
                        'ciphertext_bytes' => $expected, 'checksum' => $data['checksum'], 'offset' => 0, 'parts' => [], 'state' => 'receiving', 'expires_at' => now()->addSeconds((int) config('filebeam.staging.ttl_seconds')),
                    ]);
                    // A stage is never acknowledged until its private, lockable file exists.
                    $this->staging->create($stage);
                    $created = $stage;

                    return $stage;
                });
            });
        } catch (Throwable $exception) {
            if ($created instanceof TransferChunkStage) {
                try {
                    $this->staging->remove($created);
                } catch (RuntimeException) {
                    // The orphan sweep will retry if a failed rollback leaves a busy sidecar.
                }
            }

            if ($exception instanceof RuntimeException && ! $exception instanceof HttpException) {
                return response()->json(['message' => 'Staging storage is unavailable.'], 503, ['Retry-After' => '2']);
            }

            throw $exception;
        }

        return $this->response($stage);
    }

    /** @throws Throwable */
    public function show(Request $request, Transfer $transfer, TransferItem $item, int $position, string $upload): JsonResponse
    {
        $this->authorize($transfer, $request);
        $stage = $this->stage($transfer, $item, $position, $upload, recoverFinalizing: true);

        return $this->response($stage);
    }

    /** @throws Throwable */
    public function part(Request $request, Transfer $transfer, TransferItem $item, int $position, string $upload, int $offset): JsonResponse
    {
        $this->authorize($transfer, $request);
        $stage = $this->stage($transfer, $item, $position, $upload, validatePrefix: false);
        abort_unless($stage->state === 'receiving', 423, 'Upload is busy.');
        $body = $this->readPart($request, (int) config('filebeam.staging.part_max_bytes'));
        $size = strlen($body);
        $checksum = strtolower((string) $request->header('X-Filebeam-Part-Checksum'));
        abort_unless(preg_match('/^[a-f0-9]{64}$/', $checksum) === 1 && hash_equals($checksum, hash('sha256', $body)), 422, 'Invalid part checksum.');
        abort_unless($size > 0 && $size <= (int) config('filebeam.staging.part_max_bytes'), 422, 'Invalid part size.');

        $lock = $this->staging->lock($stage);
        try {
            if (! flock($lock, LOCK_EX | LOCK_NB)) {
                return response()->json(['message' => 'Upload is busy.'], 423, ['Retry-After' => '1']);
            }
            $stage->refresh();
            $this->ensureLive($stage);
            $this->liveTransfer($stage->transfer_id);
            abort_unless($stage->state === 'receiving', 423, 'Upload is busy.');
            $handle = $this->staging->open($stage);
            $stat = fstat($handle);
            abort_unless(is_array($stat) && $stat['size'] >= $stage->offset, 410, 'Upload staging was lost.');
            $parts = $this->parts($stage);
            foreach ($parts as $part) {
                if ($part['offset'] === $offset && $part['size'] === $size && hash_equals($part['checksum'], $checksum)) {
                    abort_unless($this->partMatches($handle, $part), 410, 'Upload staging was lost.');

                    return $this->response($stage);
                }
            }
            abort_unless($offset === $stage->offset, 409, 'Part offset conflicts with staged data.');
            abort_unless($size >= (int) config('filebeam.staging.part_min_bytes') || $offset + $size === $stage->ciphertext_bytes, 422, 'Invalid part size.');
            abort_unless($offset + $size <= $stage->ciphertext_bytes && count($parts) < (int) config('filebeam.staging.part_max_count'), 422, 'Invalid part range.');
            // A prior process can have written beyond the durable DB offset before it died.
            $written = ftruncate($handle, $stage->offset) && fseek($handle, $offset) === 0 && $this->write($handle, $body) && fflush($handle);
            if (! $written || (function_exists('fsync') && ! fsync($handle))) {
                ftruncate($handle, $stage->offset);
                abort(503, 'Staging storage is unavailable.');
            }
            $parts[] = ['offset' => $offset, 'size' => $size, 'checksum' => $checksum];
            $this->extendTransfer($stage->transfer_id);
            $stage->update(['offset' => $offset + $size, 'parts' => $parts, 'expires_at' => now()->addSeconds((int) config('filebeam.staging.ttl_seconds'))]);
        } finally {
            if (isset($handle) && is_resource($handle)) {
                fclose($handle);
            }
            flock($lock, LOCK_UN);
            fclose($lock);
        }

        return $this->response($stage->refresh());
    }

    /** @throws Throwable */
    public function complete(Request $request, Transfer $transfer, TransferItem $item, int $position, string $upload): JsonResponse
    {
        $this->authorize($transfer, $request);
        $stage = $this->stage($transfer, $item, $position, $upload);
        $lock = $this->staging->lock($stage);
        try {
            if (! flock($lock, LOCK_EX | LOCK_NB)) {
                return response()->json(['message' => 'Upload is busy.'], 423, ['Retry-After' => '1']);
            }
            $stage->refresh();
            if ($stage->state === 'finalizing') {
                $this->recoverFinalizing($stage, $transfer, $item, $position);
            }
            if ($stage->state === 'complete') {
                return $this->response($stage);
            }
            $this->ensureLive($stage);
            $this->liveTransfer($stage->transfer_id);
            abort_unless($stage->state === 'receiving', 423, 'Upload is busy.');
            $handle = $this->staging->open($stage);
            $stat = fstat($handle);
            abort_unless($this->prefixIsValid($stage, $handle), 410, 'Upload staging was lost.');
            abort_unless($stage->offset === $stage->ciphertext_bytes && is_array($stat) && $stat['size'] === $stage->ciphertext_bytes, 409, 'Staged ciphertext has a gap.');
            rewind($handle);
            $checksum = $this->hash($handle);
            abort_unless(hash_equals($stage->checksum, $checksum), 422, 'Staged ciphertext checksum does not match.');
            $stage->update(['state' => 'finalizing']);
            rewind($handle);
            $published = $this->chunks->publish($request, $transfer, $item, $position, $handle, $stage->ciphertext_bytes, $checksum);
            if ($published->getStatusCode() >= Response::HTTP_BAD_REQUEST) {
                return $published;
            }
            $stage->update(['state' => 'complete', 'offset' => $stage->ciphertext_bytes]);
        } catch (Throwable $exception) {
            if ($stage->exists && $stage->state === 'finalizing') {
                $stage->refresh();
                if ($stage->state === 'finalizing') {
                    $stage->update(['state' => 'receiving']);
                }
            }
            if ($exception instanceof RuntimeException && ! $exception instanceof HttpException) {
                return response()->json(['message' => 'Staging storage is unavailable.'], 503, ['Retry-After' => '2']);
            }

            throw $exception;
        } finally {
            if (isset($handle) && is_resource($handle)) {
                fclose($handle);
            }
            flock($lock, LOCK_UN);
            fclose($lock);
        }
        $this->staging->remove($stage);
        $stage->update(['released_at' => now()]);

        return $this->response($stage->refresh());
    }

    public function destroy(Request $request, Transfer $transfer, TransferItem $item, int $position, string $upload): Response
    {
        $this->authorize($transfer, $request);
        $stage = $this->stage($transfer, $item, $position, $upload);
        $lock = $this->staging->lock($stage);
        try {
            if (! flock($lock, LOCK_EX | LOCK_NB)) {
                return response()->json(['message' => 'Upload is busy.'], 423, ['Retry-After' => '1']);
            }
            $stage->refresh();
            abort_unless($stage->state !== 'finalizing', 423, 'Upload is busy.');
            $this->staging->remove($stage, locked: true);
            $stage->delete();
        } finally {
            flock($lock, LOCK_UN);
            fclose($lock);
        }

        return response()->noContent();
    }

    private function stage(Transfer $transfer, TransferItem $item, int $position, string $upload, bool $recoverFinalizing = false, bool $validatePrefix = true): TransferChunkStage
    {
        $stage = TransferChunkStage::query()->find($upload);
        if ($stage === null) {
            abort(410, 'Upload staging was lost.');
        }
        abort_unless($stage->transfer_id === $transfer->id && $stage->transfer_item_id === $item->id && $stage->position === $position, 404);
        $chunk = $item->chunks()->atPosition($position)->first();
        if ($chunk !== null && ! hash_equals($chunk->checksum, $stage->checksum)) {
            abort(409, 'Chunk position already contains different ciphertext.');
        }
        if ($recoverFinalizing && $stage->state === 'finalizing') {
            $lock = $this->staging->lock($stage);
            try {
                if (flock($lock, LOCK_EX | LOCK_NB)) {
                    $stage->refresh();
                    if ($stage->state === 'finalizing') {
                        $this->recoverFinalizing($stage, $transfer, $item, $position);
                    }
                    flock($lock, LOCK_UN);
                }
            } finally {
                fclose($lock);
            }
        }
        $published = $this->publishedStage($transfer, $item, $position, $stage->checksum);
        if ($published && $stage->state !== 'complete') {
            $stage->update(['state' => 'complete', 'offset' => $stage->ciphertext_bytes, 'released_at' => $this->staging->exists($stage) ? null : now()]);
        }
        if ($stage->state === 'complete' && ! $published) {
            abort(410, 'Upload staging was lost.');
        }
        if ($stage->state !== 'complete') {
            $this->ensureLive($stage);
            if ($validatePrefix) {
                $this->assertPrefix($stage);
            }
        }
        if ($stage->state !== 'complete' && ! $this->staging->exists($stage)) {
            abort(410, 'Upload staging was lost.');
        }

        return $stage;
    }

    private function recoverFinalizing(TransferChunkStage $stage, Transfer $transfer, TransferItem $item, int $position): void
    {
        $chunk = $item->chunks()->atPosition($position)->first();
        if ($chunk !== null && ! hash_equals($chunk->checksum, $stage->checksum)) {
            abort(409, 'Chunk position already contains different ciphertext.');
        }
        if ($this->publishedStage($transfer, $item, $position, $stage->checksum)) {
            $stage->update(['state' => 'complete', 'offset' => $stage->ciphertext_bytes, 'released_at' => $this->staging->exists($stage) ? null : now()]);

            return;
        }
        $stage->update(['state' => 'receiving']);
    }

    private function publishedStage(Transfer $transfer, TransferItem $item, int $position, string $checksum): bool
    {
        $chunk = $item->chunks()->atPosition($position)->first();

        return $chunk !== null && hash_equals($chunk->checksum, $checksum) && $this->published($transfer, $chunk->locations()->pluck('filestore_id')->all());
    }

    private function ensureLive(TransferChunkStage $stage): void
    {
        abort_unless($stage->expires_at->isFuture(), 410, 'Upload staging has expired.');
    }

    private function active(Transfer $transfer): void
    {
        abort_unless($transfer->status === TransferStatus::Pending && $transfer->expires_at->isFuture(), 404);
    }

    private function liveTransfer(string $transferId): void
    {
        $transfer = Transfer::query()->find($transferId);
        abort_unless($transfer !== null, 404);
        $this->active($transfer);
    }

    private function authorize(Transfer $transfer, Request $request): void
    {
        abort_unless(Capability::matches($transfer->upload_token_hash, $request->header('X-Filebeam-Upload-Token')), 403);
    }

    private function expectedBytes(Transfer $transfer, TransferItem $item, int $position): int
    {
        return min($transfer->chunk_bytes + 16, $item->declared_ciphertext_bytes - ($position * ($transfer->chunk_bytes + 16)));
    }

    /** @return list<int> */
    private function requiredLocations(Transfer $transfer): array
    {
        return $transfer->filestore_ids ?? [];
    }

    /** @param array<mixed> $locations */
    private function published(Transfer $transfer, array $locations): bool
    {
        $required = $this->requiredLocations($transfer);

        return $transfer->placement_mode === 'replicate'
            ? $required !== [] && array_diff($required, $locations) === []
            : count($locations) === 1 && array_diff($locations, $required) === [];
    }

    private function extendTransfer(string $transferId): void
    {
        DB::transaction(function () use ($transferId): void {
            $transfer = Transfer::query()->lockForUpdate()->find($transferId);
            if ($transfer === null) {
                abort(404);
            }
            $this->active($transfer);
            $maximum = $transfer->created_at->addHours((int) config('filebeam.transfers.pending_max_lifetime_hours'));
            $extended = now()->addHours((int) config('filebeam.transfers.incomplete_expiry_hours'));
            $expiresAt = $extended->lessThan($maximum) ? $extended : $maximum;
            if ($expiresAt->greaterThan($transfer->expires_at)) {
                $transfer->update(['expires_at' => $expiresAt]);
            }
        });
    }

    private function readPart(Request $request, int $maximum): string
    {
        $length = $request->header('Content-Length');
        abort_if(is_string($length) && (! ctype_digit($length) || (int) $length > $maximum), 422, 'Invalid part size.');
        $input = $request->getContent(asResource: true);
        $body = '';
        try {
            while (! feof($input)) {
                $remaining = $maximum + 1 - strlen($body);
                abort_if($remaining < 1, 422, 'Invalid part size.');
                $chunk = fread($input, min(1_048_576, $remaining));
                if ($chunk === false) {
                    throw new RuntimeException('Unable to read staging part.');
                }
                $body .= $chunk;
                abort_if(strlen($body) > $maximum, 422, 'Invalid part size.');
            }
        } finally {
            fclose($input);
        }

        return $body;
    }

    private function validUuid(string $id): bool
    {
        return preg_match('/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i', $id) === 1;
    }

    /** @return list<array{offset: int, size: int, checksum: string}> */
    private function parts(TransferChunkStage $stage): array
    {
        $parts = json_decode((string) $stage->getRawOriginal('parts'), true);

        if (! is_array($parts)) {
            return [];
        }

        return array_values(array_filter($parts, fn (mixed $part): bool => is_array($part)
            && is_int($part['offset'] ?? null) && is_int($part['size'] ?? null) && is_string($part['checksum'] ?? null)));
    }

    /** @param resource $handle */
    private function write($handle, string $body): bool
    {
        $at = 0;
        while ($at < strlen($body)) {
            $written = fwrite($handle, substr($body, $at));
            if ($written === false || $written === 0) {
                return false;
            }
            $at += $written;
        }

        return true;
    }

    /** @param resource $handle */
    private function hash($handle): string
    {
        rewind($handle);
        $context = hash_init('sha256');
        while (! feof($handle)) {
            $chunk = fread($handle, 1_048_576);
            if ($chunk === false) {
                throw new RuntimeException('Unable to read staged ciphertext.');
            }
            hash_update($context, $chunk);
        }

        return hash_final($context);
    }

    private function assertPrefix(TransferChunkStage $stage): void
    {
        abort_unless($this->staging->exists($stage), 410, 'Upload staging was lost.');
        $handle = $this->staging->open($stage);
        try {
            abort_unless($this->prefixIsValid($stage, $handle), 410, 'Upload staging was lost.');
        } finally {
            fclose($handle);
        }
    }

    /** @param resource $handle */
    private function prefixIsValid(TransferChunkStage $stage, $handle): bool
    {
        $stat = fstat($handle);
        if (! is_array($stat) || $stat['size'] < $stage->offset) {
            return false;
        }
        $offset = 0;
        foreach ($this->parts($stage) as $part) {
            if ($part['offset'] !== $offset || ! $this->partMatches($handle, $part)) {
                return false;
            }
            $offset += $part['size'];
        }

        return $offset === $stage->offset;
    }

    /** @param resource $handle
     * @param  array{offset: int, size: int, checksum: string}  $part
     */
    private function partMatches($handle, array $part): bool
    {
        if (fseek($handle, $part['offset']) !== 0) {
            return false;
        }
        $remaining = $part['size'];
        $hash = hash_init('sha256');
        while ($remaining > 0) {
            $chunk = fread($handle, min(1_048_576, $remaining));
            if ($chunk === false || $chunk === '') {
                return false;
            }
            $remaining -= strlen($chunk);
            hash_update($hash, $chunk);
        }

        return hash_equals($part['checksum'], hash_final($hash));
    }

    private function response(TransferChunkStage $stage): JsonResponse
    {
        $status = $stage->state === 'complete' ? 200 : ($stage->state === 'finalizing' ? 202 : 201);
        $headers = $stage->state === 'finalizing' ? ['Retry-After' => '1'] : [];

        return response()->json(['data' => ['id' => $stage->id, 'state' => $stage->state, 'offset' => $stage->offset, 'ciphertext_bytes' => $stage->ciphertext_bytes, 'checksum' => $stage->checksum]], $status, $headers);
    }
}
