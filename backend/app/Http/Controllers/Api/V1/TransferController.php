<?php

declare(strict_types=1);

namespace App\Http\Controllers\Api\V1;

use App\Enums\TransferDelivery;
use App\Enums\TransferDriver;
use App\Enums\TransferKind;
use App\Enums\TransferStatus;
use App\Http\Controllers\Controller;
use App\Http\Requests\CompleteTransferRequest;
use App\Http\Requests\StoreTransferRequest;
use App\Http\Resources\TransferResource;
use App\Jobs\DeleteTransfer;
use App\Models\AccountKeyBundle;
use App\Models\Filestore;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferChunk;
use App\Models\TransferItem;
use App\Models\TransferKeyEnvelope;
use App\Models\User;
use App\Notifications\InboxTransferCompleted;
use App\Support\Capability;
use App\Support\ChunkStaging;
use App\Support\EffectivePlan;
use App\Support\FilestoreRegistry;
use App\Support\InstanceSettings;
use App\Support\TransportPolicy;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Str;
use Illuminate\Validation\ValidationException;
use Throwable;

class TransferController extends Controller
{
    /**
     * @throws Throwable
     */
    public function store(StoreTransferRequest $request, EffectivePlan $plans, FilestoreRegistry $stores, TransportPolicy $transport): JsonResponse
    {
        /** @var array{kind: string, driver?: string, protocol_version: int, chunk_bytes: int, items: array<int, array{ciphertext_bytes: int, chunk_count: int}>, retention_hours?: int, burn_on_read?: bool, recipient_username?: string, account_key_bundle_id?: int} $validated */
        $validated = $request->validated();
        $plan = $plans->resolve($request->user());
        $kind = TransferKind::from($validated['kind']);
        $driver = TransferDriver::from((string) $request->input('driver', TransferDriver::Http->value));
        abort_unless($transport->allows($driver), 422);
        $recipientUsername = $validated['recipient_username'] ?? null;
        $recipient = null;
        $recipientBundle = null;
        if ($recipientUsername !== null || isset($validated['account_key_bundle_id'])) {
            abort_unless(app(InstanceSettings::class)->boolean('username_routing'), 404);
            if ($kind !== TransferKind::Files || ($validated['burn_on_read'] ?? false)) {
                throw ValidationException::withMessages(['recipient_username' => 'Inbox delivery requires files without burn on read.']);
            }
            $recipient = User::query()
                ->where('normalized_username', $recipientUsername)
                ->inboxEnabled()
                ->first();
            $recipientBundle = $recipient?->accountKeyBundles()->whereKey($validated['account_key_bundle_id'] ?? 0)->active()->first();
            if ($recipient === null || $recipientBundle === null) {
                throw ValidationException::withMessages(['recipient_username' => 'The receiving inbox is unavailable.']);
            }
        }
        $itemCount = count($validated['items']);
        $declaredBytes = 0;
        foreach ($validated['items'] as $item) {
            if ($item['ciphertext_bytes'] > PHP_INT_MAX - $declaredBytes) {
                throw ValidationException::withMessages(['items' => 'The declared ciphertext total is too large.']);
            }
            $declaredBytes += $item['ciphertext_bytes'];
        }
        $retentionHours = $validated['retention_hours'] ?? ($kind === TransferKind::Note
            ? $plan->default_note_retention_hours
            : $plan->default_file_retention_hours);
        $burnOnRead = $kind === TransferKind::Note && ($validated['burn_on_read'] ?? false);

        $this->validatePlanLimits($plan, $kind, $driver, $itemCount, $declaredBytes, $transport);

        $uploadToken = Str::random(64);
        $deleteToken = Str::random(64);
        $readToken = $burnOnRead ? Str::random(64) : null;
        $monitorToken = $kind === TransferKind::Files && $recipient === null ? Str::random(64) : null;
        $joinToken = $driver === TransferDriver::WebRtc ? Str::random(64) : null;
        $transferId = (string) Str::ulid();
        /** @var Transfer $transfer */
        $transfer = DB::transaction(function () use (
            $request,
            $validated,
            $plan,
            $kind,
            $itemCount,
            $declaredBytes,
            $retentionHours,
            $burnOnRead,
            $uploadToken,
            $deleteToken,
            $readToken,
            $monitorToken,
            $transferId,
            $recipient,
            $recipientBundle,
            $recipientUsername,
            $stores,
            $driver,
            $joinToken,
            $transport,
        ): Transfer {
            $plan = Plan::query()->lockForUpdate()->findOrFail($plan->id);
            $this->validatePlanLimits($plan, $kind, $driver, $itemCount, $declaredBytes, $transport);
            $filestoreIds = $driver === TransferDriver::Http ? $stores->selectForPlan($plan, null) : [];
            if ($driver === TransferDriver::Http) {
                $selected = Filestore::query()->whereKey($filestoreIds)->orderBy('id')->lockForUpdate()->get();
                if ($selected->count() !== count($filestoreIds) || $selected->contains(fn (Filestore $store): bool => ! $stores->placementEnabled($store))) {
                    throw ValidationException::withMessages(['filestore_ids' => 'The selected filestores are no longer available.']);
                }
            }
            $lockedRecipient = null;
            $lockedRecipientBundle = null;
            if ($recipient !== null) {
                $lockedRecipient = User::query()->lockForUpdate()->find($recipient->id);
                $lockedRecipientBundle = AccountKeyBundle::query()->lockForUpdate()->findOrFail($recipientBundle->id);
                if (! app(InstanceSettings::class)->boolean('username_routing')
                    || $lockedRecipient === null
                    || $lockedRecipient->suspended_at !== null
                    || ! $lockedRecipient->inbox_enabled
                    || $lockedRecipient->normalized_username !== $recipientUsername
                    || $lockedRecipientBundle->user_id !== $lockedRecipient->id
                    || ! $lockedRecipientBundle->is_active) {
                    throw ValidationException::withMessages(['recipient_username' => 'The receiving inbox is unavailable.']);
                }
            }
            $transfer = Transfer::query()->create([
                'id' => $transferId,
                'kind' => $kind,
                'delivery' => $lockedRecipient === null ? TransferDelivery::Link : TransferDelivery::Inbox,
                'driver' => $driver,
                'owner_id' => $request->user()?->getKey(),
                'recipient_id' => $lockedRecipient?->getKey(),
                'plan_id' => $plan->getKey(),
                'filestore_ids' => $filestoreIds,
                'placement_mode' => $plan->placement_mode,
                'status' => TransferStatus::Pending,
                'protocol_version' => $validated['protocol_version'],
                'chunk_bytes' => $validated['chunk_bytes'],
                'retention_hours' => $retentionHours,
                'burn_on_read' => $burnOnRead,
                'declared_ciphertext_bytes' => $declaredBytes,
                'item_count' => $itemCount,
                'upload_token_hash' => hash('sha256', $uploadToken),
                'delete_token_hash' => hash('sha256', $deleteToken),
                'read_token_hash' => $readToken === null ? null : hash('sha256', $readToken),
                'monitor_token_hash' => $monitorToken === null ? null : hash('sha256', $monitorToken),
                'join_token_hash' => $joinToken === null ? null : hash('sha256', $joinToken),
                'expires_at' => now()->addHours((int) config('filebeam.transfers.incomplete_expiry_hours')),
            ]);

            $items = [];
            $createdAt = now();

            foreach ($validated['items'] as $position => $item) {
                $itemId = (string) Str::ulid();
                $items[] = [
                    'id' => $itemId,
                    'transfer_id' => $transferId,
                    'position' => $position,
                    'chunk_count' => $item['chunk_count'],
                    'declared_ciphertext_bytes' => $item['ciphertext_bytes'],
                    'created_at' => $createdAt,
                    'updated_at' => $createdAt,
                ];
            }

            // Stay below SQLite's bind-parameter limit even on older builds.
            foreach (array_chunk($items, 50) as $batch) {
                TransferItem::query()->insert($batch);
            }

            if ($lockedRecipientBundle instanceof AccountKeyBundle) {
                TransferKeyEnvelope::query()->create([
                    'transfer_id' => $transfer->id,
                    'account_key_bundle_id' => $lockedRecipientBundle->id,
                    'role' => 'recipient',
                    'encrypted_key' => '',
                ]);
            }

            return $transfer->load('items');
        });

        return response()->json([
            'data' => [
                'id' => $transfer->id,
                'share_url' => route('transfers.show', ['transferId' => $transfer->id], absolute: false),
                'expires_at' => $transfer->expires_at->toIso8601String(),
                'chunk_bytes' => $transfer->chunk_bytes,
                'upload_transport' => $transfer->driver === TransferDriver::Http ? app(ChunkStaging::class)->transport($transfer->chunk_bytes) : null,
                'transfer_capabilities' => ['upload_status' => true, 'download_ranges' => true],
                'driver' => $transfer->driver->value,
                'items' => $transfer->items->map(fn (TransferItem $item): array => [
                    'id' => $item->id,
                    'position' => $item->position,
                ])->all(),
                'upload_token' => $uploadToken,
                'delete_token' => $deleteToken,
                ...($readToken === null ? [] : ['read_token' => $readToken]),
                ...($monitorToken === null ? [] : ['monitor_token' => $monitorToken]),
                ...($joinToken === null ? [] : ['join_token' => $joinToken]),
            ],
        ], 201, ['Cache-Control' => 'no-store']);
    }

    private function validatePlanLimits(Plan $plan, TransferKind $kind, TransferDriver $driver, int $itemCount, int $declaredBytes, TransportPolicy $transport): void
    {
        $errors = [];

        $limits = $transport->limits($plan, $driver);
        if ($limits['maximum_file_count'] !== null && $itemCount > $limits['maximum_file_count']) {
            $errors['items'] = "This plan permits at most {$limits['maximum_file_count']} items.";
        }

        if ($kind === TransferKind::Note && $itemCount !== 1) {
            $errors['items'] = 'A note transfer must contain exactly one item.';
        }

        $maximumBytes = $kind === TransferKind::Note ? $limits['maximum_note_bytes'] : $limits['maximum_transfer_bytes'];

        if ($maximumBytes !== null && $declaredBytes > $maximumBytes) {
            $errors['items'] = 'The transfer exceeds this plan\'s byte limit.';
        }

        if ($errors !== []) {
            throw ValidationException::withMessages($errors);
        }
    }

    public function show(Transfer $transfer): TransferResource
    {
        abort_unless(
            $transfer->delivery === TransferDelivery::Link
                && ($transfer->status === TransferStatus::Available || $transfer->status === TransferStatus::Live || ($transfer->status === TransferStatus::Pending && $transfer->isPublishedTurbo()))
                && $transfer->expires_at->isFuture(),
            404,
        );

        return new TransferResource($transfer->load('items'));
    }

    public function uploadStatus(Request $request, Transfer $transfer): JsonResponse
    {
        $this->authorizeCapability($transfer->upload_token_hash, $request->header('X-Filebeam-Upload-Token'));
        $data = $request->validate([
            'after' => ['nullable', 'string', 'max:128'],
            'limit' => ['nullable', 'integer', 'min:1', 'max:500'],
        ]);
        abort_unless(
            $transfer->driver === TransferDriver::Http
                && in_array($transfer->status, [TransferStatus::Pending, TransferStatus::Available], true)
                && $transfer->expires_at->isFuture(),
            404,
        );
        $cursor = isset($data['after']) ? $this->decodeUploadCursor($data['after']) : null;
        $limit = $data['limit'] ?? 500;
        $required = $transfer->filestore_ids ?? [];
        $chunks = TransferChunk::query()
            ->select('transfer_chunks.*')
            ->join('transfer_items', 'transfer_chunks.transfer_item_id', '=', 'transfer_items.id')
            ->where('transfer_items.transfer_id', $transfer->id)
            ->when($required === [], fn ($query) => $query->whereIn('transfer_chunks.id', []))
            ->when($transfer->placement_mode === 'replicate', function ($query) use ($required): void {
                foreach ($required as $filestoreId) {
                    $query->whereHas('locations', fn ($locations) => $locations->where('filestore_id', $filestoreId));
                }
            })
            ->when($transfer->placement_mode !== 'replicate', fn ($query) => $query->has('locations', '=', 1)
                ->whereHas('locations', fn ($locations) => $locations->whereIn('filestore_id', $required)))
            ->when($cursor !== null, fn ($query) => $query->where(function ($query) use ($cursor): void {
                $query->where('transfer_items.position', '>', $cursor['item'])
                    ->orWhere(function ($query) use ($cursor): void {
                        $query->where('transfer_items.position', $cursor['item'])
                            ->where('transfer_chunks.position', '>', $cursor['chunk']);
                    });
            }))
            ->with('item:id,transfer_id,position')
            ->orderBy('transfer_items.position')
            ->orderBy('transfer_chunks.position')
            ->limit($limit + 1)
            ->get();
        $next = $chunks->count() > $limit ? $chunks->pop() : null;

        return response()->json(['data' => [
            'id' => $transfer->id,
            'status' => $transfer->status->value,
            'protocol_version' => $transfer->protocol_version,
            'chunk_bytes' => $transfer->chunk_bytes,
            'expires_at' => $transfer->expires_at->toIso8601String(),
            'upload_transport' => app(ChunkStaging::class)->transport($transfer->chunk_bytes),
            'items' => $transfer->items()->get(['id', 'position', 'chunk_count', 'declared_ciphertext_bytes'])->map(fn (TransferItem $item): array => [
                'id' => $item->id, 'position' => $item->position, 'chunk_count' => $item->chunk_count,
                'declared_ciphertext_bytes' => $item->declared_ciphertext_bytes,
            ])->all(),
            'chunks' => $chunks->map(fn (TransferChunk $chunk): array => [
                'item_id' => $chunk->transfer_item_id, 'position' => $chunk->position,
                'ciphertext_bytes' => $chunk->ciphertext_bytes, 'checksum' => $chunk->checksum,
            ])->all(),
            'next_cursor' => $next === null ? null : $this->encodeUploadCursor($chunks->last()),
        ]], 200, ['Cache-Control' => 'no-store, private']);
    }

    /** @return array{item: int, chunk: int} */
    private function decodeUploadCursor(string $cursor): array
    {
        $decoded = base64_decode(strtr($cursor, '-_', '+/'), true);
        $value = $decoded === false ? null : json_decode($decoded, true);
        abort_unless(is_array($value) && isset($value['i'], $value['p']) && is_int($value['i']) && is_int($value['p']) && $value['i'] >= 0 && $value['p'] >= 0, 422);

        return ['item' => $value['i'], 'chunk' => $value['p']];
    }

    private function encodeUploadCursor(TransferChunk $chunk): string
    {
        $item = $chunk->item;
        assert($item instanceof TransferItem);

        return rtrim(strtr(base64_encode(json_encode(['i' => $item->position, 'p' => $chunk->position], JSON_THROW_ON_ERROR)), '+/', '-_'), '=');
    }

    /**
     * @throws Throwable
     */
    public function complete(CompleteTransferRequest $request, Transfer $transfer): TransferResource
    {
        /** @var array{encrypted_manifest: string, encrypted_key?: string} $validated */
        $validated = $request->validated();

        abort_unless($transfer->driver === TransferDriver::Http, 404);

        $notifyRecipient = DB::transaction(function () use ($request, $transfer, $validated): ?User {
            $lockedTransfer = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
            $this->authorizeCapability($lockedTransfer->upload_token_hash, $request->header('X-Filebeam-Upload-Token'));
            abort_unless($lockedTransfer->expires_at->isFuture(), 404);

            if ($lockedTransfer->delivery === TransferDelivery::Inbox) {
                abort_unless(app(InstanceSettings::class)->boolean('username_routing'), 404);
            }

            if ($lockedTransfer->status === TransferStatus::Available) {
                abort_unless(
                    is_string($lockedTransfer->encrypted_manifest)
                        && hash_equals($lockedTransfer->encrypted_manifest, $validated['encrypted_manifest']),
                    409,
                    'Transfer was already completed with a different manifest.',
                );

                if ($lockedTransfer->delivery === TransferDelivery::Inbox) {
                    $envelope = $lockedTransfer->keyEnvelopes()->recipient()->sole();
                    abort_unless(isset($validated['encrypted_key']) && hash_equals($envelope->encrypted_key, $validated['encrypted_key']), 409);
                }

                return null;
            }

            abort_unless($lockedTransfer->status === TransferStatus::Pending, 409, 'Transfer is not pending.');

            if ($lockedTransfer->delivery === TransferDelivery::Inbox) {
                $recipient = User::query()->lockForUpdate()->find($lockedTransfer->recipient_id);
                $envelope = $lockedTransfer->keyEnvelopes()->recipient()->lockForUpdate()->sole();
                $bundle = AccountKeyBundle::query()->lockForUpdate()->find($envelope->account_key_bundle_id);
                abort_unless(
                    $recipient !== null
                        && $recipient->suspended_at === null
                        && $recipient->inbox_enabled
                        && $bundle !== null
                        && $bundle->user_id === $recipient->id
                        && $bundle->is_active
                        && is_string($validated['encrypted_key'] ?? null),
                    409,
                    'The recipient key is unavailable.',
                );
                $envelope->update(['encrypted_key' => $validated['encrypted_key']]);
            }

            $items = $lockedTransfer->items()->lockForUpdate()->withCount('chunks')->get();
            $isComplete = $items->count() === $lockedTransfer->item_count
                && $items->every(fn (TransferItem $item): bool => $item->chunks_count === $item->chunk_count
                    && $item->ciphertext_bytes === $item->declared_ciphertext_bytes);

            abort_unless($isComplete, 409, 'All declared chunks must be uploaded before completion.');

            foreach ($lockedTransfer->chunks()->with('locations')->lazyById(100, 'transfer_chunks.id', 'id') as $chunk) {
                $locations = $chunk->locations->pluck('filestore_id')->all();
                $required = $lockedTransfer->filestore_ids ?? [];
                $copiesComplete = $lockedTransfer->placement_mode === 'replicate'
                    ? $required !== [] && array_diff($required, $locations) === []
                    : count($locations) === 1 && array_diff($locations, $required) === [];
                abort_unless($copiesComplete, 409, 'All required chunk copies must be uploaded before completion.');
            }

            $completedAt = now();
            $lockedTransfer->update([
                'encrypted_manifest' => $validated['encrypted_manifest'],
                'ciphertext_bytes' => $items->sum('ciphertext_bytes'),
                'status' => TransferStatus::Available,
                'completed_at' => $completedAt,
                'expires_at' => $completedAt->addHours($lockedTransfer->retention_hours),
            ]);

            $lockedTransfer->items()->update(['completed_at' => $completedAt]);

            return $lockedTransfer->delivery === TransferDelivery::Inbox ? $lockedTransfer->recipient : null;
        });

        if ($notifyRecipient instanceof User) {
            $notifyRecipient->notify(new InboxTransferCompleted);
        }

        return new TransferResource($transfer->refresh()->load('items'));
    }

    private function authorizeCapability(string $expectedHash, ?string $token): void
    {
        abort_unless(
            Capability::matches($expectedHash, $token),
            403,
        );
    }

    /**
     * @throws Throwable
     */
    public function destroy(Transfer $transfer): JsonResponse
    {
        DB::transaction(function () use ($transfer): void {
            $lockedTransfer = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
            $this->authorizeCapability($lockedTransfer->delete_token_hash, request()->header('X-Filebeam-Delete-Token'));

            if ($lockedTransfer->status !== TransferStatus::Deleting) {
                $lockedTransfer->update(['status' => TransferStatus::Deleting]);
                DB::afterCommit(fn (): mixed => DeleteTransfer::dispatch($lockedTransfer->id));
            }
        });

        if ($transfer->driver === TransferDriver::WebRtc) {
            Cache::forget("filebeam:webrtc:{$transfer->id}:sessions");
            Cache::forget("filebeam:webrtc:{$transfer->id}:sender");
        }

        return response()->json(status: 202);
    }

    /**
     * @throws Throwable
     */
    public function consume(Transfer $transfer): JsonResponse
    {
        if ($transfer->driver === TransferDriver::WebRtc) {
            Cache::lock("filebeam:webrtc:{$transfer->id}:lock", 5)->block(3, function () use ($transfer): void {
                DB::transaction(function () use ($transfer): void {
                    $lockedTransfer = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
                    $this->authorizeCapability($lockedTransfer->read_token_hash ?? '', request()->header('X-Filebeam-Read-Token'));
                    abort_unless(
                        $lockedTransfer->kind === TransferKind::Note
                            && $lockedTransfer->delivery === TransferDelivery::Link
                            && $lockedTransfer->burn_on_read
                            && $lockedTransfer->status === TransferStatus::Live
                            && $lockedTransfer->expires_at->isFuture(),
                        404,
                    );
                    $sessionToken = request()->header('X-Filebeam-Session-Token');
                    $sessions = Cache::get("filebeam:webrtc:{$lockedTransfer->id}:sessions", []);
                    $claimed = collect(is_array($sessions) ? $sessions : [])->firstWhere('id', $lockedTransfer->webrtc_claimed_session_id);
                    abort_unless(
                        is_array($claimed)
                            && is_int($claimed['expires_at'] ?? null)
                            && $claimed['expires_at'] > now()->getTimestamp()
                            && is_string($sessionToken)
                            && Capability::matches((string) ($claimed['token_hash'] ?? ''), $sessionToken),
                        403,
                    );
                    $lockedTransfer->update(['status' => TransferStatus::Deleting]);
                    DB::afterCommit(fn (): mixed => DeleteTransfer::dispatch($lockedTransfer->id));
                });
                Cache::forget("filebeam:webrtc:{$transfer->id}:sessions");
                Cache::forget("filebeam:webrtc:{$transfer->id}:sender");
            });

            return response()->json(status: 202);
        }

        DB::transaction(function () use ($transfer): void {
            $lockedTransfer = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
            $this->authorizeCapability($lockedTransfer->read_token_hash ?? '', request()->header('X-Filebeam-Read-Token'));
            abort_unless(
                $lockedTransfer->kind === TransferKind::Note
                    && $lockedTransfer->delivery === TransferDelivery::Link
                    && $lockedTransfer->burn_on_read
                    && $lockedTransfer->expires_at->isFuture(),
                404,
            );

            if ($lockedTransfer->status !== TransferStatus::Deleting) {
                abort_unless(in_array($lockedTransfer->status, [TransferStatus::Available, TransferStatus::Live], true), 404);
                $lockedTransfer->update(['status' => TransferStatus::Deleting]);
                DB::afterCommit(fn (): mixed => DeleteTransfer::dispatch($lockedTransfer->id));
            }
        });

        return response()->json(status: 202);
    }
}
