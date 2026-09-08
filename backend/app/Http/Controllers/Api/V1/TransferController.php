<?php

declare(strict_types=1);

namespace App\Http\Controllers\Api\V1;

use App\Enums\TransferDelivery;
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
use App\Models\TransferItem;
use App\Models\TransferKeyEnvelope;
use App\Models\User;
use App\Notifications\InboxTransferCompleted;
use App\Support\Capability;
use App\Support\ChunkStaging;
use App\Support\EffectivePlan;
use App\Support\FilestoreRegistry;
use App\Support\InstanceSettings;
use Illuminate\Http\JsonResponse;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Str;
use Illuminate\Validation\ValidationException;
use Throwable;

class TransferController extends Controller
{
    /**
     * @throws Throwable
     */
    public function store(StoreTransferRequest $request, EffectivePlan $plans, FilestoreRegistry $stores): JsonResponse
    {
        /** @var array{kind: string, protocol_version: int, chunk_bytes: int, items: array<int, array{ciphertext_bytes: int, chunk_count: int}>, retention_hours?: int, burn_on_read?: bool, recipient_username?: string, account_key_bundle_id?: int} $validated */
        $validated = $request->validated();
        $plan = $plans->resolve($request->user());
        $kind = TransferKind::from($validated['kind']);
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
        $declaredBytes = array_sum(array_column($validated['items'], 'ciphertext_bytes'));
        $retentionHours = $validated['retention_hours'] ?? ($kind === TransferKind::Note
            ? $plan->default_note_retention_hours
            : $plan->default_file_retention_hours);
        $burnOnRead = $kind === TransferKind::Note && ($validated['burn_on_read'] ?? false);

        $this->validatePlanLimits($plan, $kind, $itemCount, $declaredBytes);

        $uploadToken = Str::random(64);
        $deleteToken = Str::random(64);
        $readToken = $burnOnRead ? Str::random(64) : null;
        $monitorToken = $kind === TransferKind::Files && $recipient === null ? Str::random(64) : null;
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
        ): Transfer {
            $plan = Plan::query()->lockForUpdate()->findOrFail($plan->id);
            $filestoreIds = $stores->selectForPlan($plan, null);
            $selected = Filestore::query()->whereKey($filestoreIds)->orderBy('id')->lockForUpdate()->get();
            if ($selected->count() !== count($filestoreIds) || $selected->contains(fn (Filestore $store): bool => ! $stores->placementEnabled($store))) {
                throw ValidationException::withMessages(['filestore_ids' => 'The selected filestores are no longer available.']);
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
                'upload_transport' => app(ChunkStaging::class)->transport($transfer->chunk_bytes),
                'items' => $transfer->items->map(fn (TransferItem $item): array => [
                    'id' => $item->id,
                    'position' => $item->position,
                ])->all(),
                'upload_token' => $uploadToken,
                'delete_token' => $deleteToken,
                ...($readToken === null ? [] : ['read_token' => $readToken]),
                ...($monitorToken === null ? [] : ['monitor_token' => $monitorToken]),
            ],
        ], 201, ['Cache-Control' => 'no-store']);
    }

    private function validatePlanLimits(Plan $plan, TransferKind $kind, int $itemCount, int $declaredBytes): void
    {
        $errors = [];

        if ($itemCount > $plan->maximum_file_count) {
            $errors['items'] = "This plan permits at most {$plan->maximum_file_count} items.";
        }

        if ($kind === TransferKind::Note && $itemCount !== 1) {
            $errors['items'] = 'A note transfer must contain exactly one item.';
        }

        $maximumBytes = $kind === TransferKind::Note
            ? $plan->maximum_note_bytes
            : $plan->maximum_transfer_bytes;

        if ($declaredBytes > $maximumBytes) {
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
                && ($transfer->status === TransferStatus::Available || ($transfer->status === TransferStatus::Pending && $transfer->isPublishedTurbo()))
                && $transfer->expires_at->isFuture(),
            404,
        );

        return new TransferResource($transfer->load('items'));
    }

    /**
     * @throws Throwable
     */
    public function complete(CompleteTransferRequest $request, Transfer $transfer): TransferResource
    {
        /** @var array{encrypted_manifest: string, encrypted_key?: string} $validated */
        $validated = $request->validated();

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

        return response()->json(status: 202);
    }

    /**
     * @throws Throwable
     */
    public function consume(Transfer $transfer): JsonResponse
    {
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
                abort_unless($lockedTransfer->status === TransferStatus::Available, 404);
                $lockedTransfer->update(['status' => TransferStatus::Deleting]);
                DB::afterCommit(fn (): mixed => DeleteTransfer::dispatch($lockedTransfer->id));
            }
        });

        return response()->json(status: 202);
    }
}
