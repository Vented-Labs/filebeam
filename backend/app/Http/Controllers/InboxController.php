<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Enums\TransferDelivery;
use App\Enums\TransferStatus;
use App\Http\Resources\TransferResource;
use App\Jobs\DeleteTransfer;
use App\Models\Transfer;
use App\Models\TransferChunk;
use App\Models\TransferItem;
use App\Models\User;
use App\Notifications\InboxTransferCompleted;
use App\Support\ChunkReader;
use App\Support\InstanceSettings;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Inertia\Inertia;
use Inertia\Response;
use Symfony\Component\HttpFoundation\StreamedResponse;

class InboxController extends Controller
{
    public function update(Request $request): JsonResponse
    {
        abort_unless(app(InstanceSettings::class)->boolean('username_routing'), 404);
        $data = $request->validate(['enabled' => ['required', 'boolean']]);
        DB::transaction(function () use ($request, $data): void {
            $user = User::query()->lockForUpdate()->findOrFail($request->user()->id);
            if ($data['enabled']) {
                abort_unless($user->accountKeyBundles()->active()->exists(), 422, 'An active account key is required.');
            }
            $user->update(['inbox_enabled' => $data['enabled']]);
        });

        return response()->json(['data' => ['enabled' => $data['enabled']]]);
    }

    public function notifications(Request $request): JsonResponse
    {
        $data = $request->validate(['channel' => ['required', 'in:mail,database']]);
        $request->user()->update(['notification_channel' => $data['channel']]);

        return response()->json(['data' => ['channel' => $data['channel']]]);
    }

    public function index(Request $request): Response
    {
        $user = $request->user();
        assert($user instanceof User);
        $user->unreadNotifications()->where('type', InboxTransferCompleted::class)->update(['read_at' => now()]);

        return Inertia::render('Inbox', ['transfers' => $this->transfers($user)]);
    }

    public function show(Request $request, Transfer $transfer): Response
    {
        $this->transfer($request, $transfer);

        return Inertia::render('InboxTransfer', ['transferId' => $transfer->id]);
    }

    public function metadata(Request $request, Transfer $transfer): JsonResponse
    {
        $this->transfer($request, $transfer);
        $envelope = $transfer->keyEnvelopes()->recipient()->with('accountKeyBundle')->sole();

        return response()->json(['data' => [...(new TransferResource($transfer->load('items')))->resolve(), 'recipient_key' => [
            'bundle' => $envelope->accountKeyBundle->only('id', 'user_id', 'version', 'public_key', 'fingerprint', 'custody_mode', 'encrypted_private_key', 'is_active'),
            'encrypted_key' => $envelope->encrypted_key,
        ]]], 200, ['Cache-Control' => 'no-store']);
    }

    public function chunk(Request $request, Transfer $transfer, TransferItem $item, int $position, ChunkReader $reader): StreamedResponse
    {
        $this->transfer($request, $transfer);
        /** @var TransferChunk $chunk */
        $chunk = $item->chunks()->atPosition($position)->firstOrFail();
        $stream = $reader->read($chunk);

        return response()->stream(function () use ($stream): void {
            try {
                fpassthru($stream);
            } finally {
                fclose($stream);
            }
        }, 200, ['Cache-Control' => 'private, no-store', 'Content-Length' => (string) $chunk->ciphertext_bytes, 'Content-Type' => 'application/octet-stream', 'X-Content-Type-Options' => 'nosniff']);
    }

    public function destroy(Request $request, Transfer $transfer): JsonResponse
    {
        $this->transfer($request, $transfer);
        DB::transaction(function () use ($transfer): void {
            $locked = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
            if ($locked->status !== TransferStatus::Deleting) {
                $locked->update(['status' => TransferStatus::Deleting]);
                DB::afterCommit(fn (): mixed => DeleteTransfer::dispatch($locked->id));
            }
        });

        return response()->json(status: 202);
    }

    /** @return list<array<string, int|string|null>> */
    private function transfers(User $user): array
    {
        $transfers = [];

        foreach ($user->receivedTransfers()->where('delivery', TransferDelivery::Inbox)->availableAndUnexpired()->latest('completed_at')->get() as $transfer) {
            $transfers[] = [
                'id' => $transfer->id,
                'ciphertext_bytes' => $transfer->ciphertext_bytes,
                'item_count' => $transfer->item_count,
                'completed_at' => $transfer->completed_at?->toIso8601String(),
                'expires_at' => $transfer->expires_at->toIso8601String(),
            ];
        }

        return $transfers;
    }

    private function transfer(Request $request, Transfer $transfer): void
    {
        abort_unless($transfer->recipient_id === $request->user()?->id && $transfer->delivery === TransferDelivery::Inbox && $transfer->status === TransferStatus::Available && $transfer->expires_at->isFuture(), 404);
    }
}
