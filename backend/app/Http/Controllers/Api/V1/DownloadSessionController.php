<?php

declare(strict_types=1);

namespace App\Http\Controllers\Api\V1;

use App\Enums\TransferStatus;
use App\Http\Controllers\Controller;
use App\Http\Requests\DownloadSessionRequest;
use App\Http\Requests\UpdateDownloadSessionRequest;
use App\Models\Transfer;
use App\Support\Capability;
use Illuminate\Http\JsonResponse;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Str;
use Symfony\Component\HttpKernel\Exception\HttpExceptionInterface;
use Throwable;

class DownloadSessionController extends Controller
{
    /**
     * @throws HttpExceptionInterface
     */
    public function store(DownloadSessionRequest $request, Transfer $transfer): JsonResponse
    {
        abort_unless($transfer->isPublishedTurbo() && $transfer->expires_at->isFuture(), 404);
        /** @var array{item_ids: list<string>} $validated */
        $validated = $request->validated();
        $itemIds = $validated['item_ids'];
        /** @var list<string> $transferItemIds */
        $transferItemIds = $transfer->items()->pluck('id')->all();
        abort_unless(count($itemIds) <= count($transferItemIds) && array_diff($itemIds, $transferItemIds) === [], 422);
        $token = Str::random(64);
        $id = (string) Str::ulid();

        $this->withinLock($transfer, function (array $sessions) use ($transfer, $itemIds, $token, $id): array {
            abort_if(count($sessions) >= (int) config('filebeam.transfers.session_limit'), 429);
            $expiresAt = $this->sessionExpiry($transfer, false);
            $sessions[] = [
                'id' => $id,
                'token_hash' => hash('sha256', $token),
                'number' => $this->nextNumber($transfer),
                'progress' => 0,
                'status' => 'waiting',
                'selection_count' => count($itemIds),
                'all_files' => count($itemIds) === $transfer->item_count,
                'sequence' => 0,
                'heartbeat_at' => now()->getTimestamp(),
                'expires_at' => $expiresAt,
            ];

            return $sessions;
        });

        return response()->json(['data' => ['id' => $id, 'token' => $token]], 201, ['Cache-Control' => 'no-store']);
    }

    /** @param callable(list<array<string, mixed>>): list<array<string, mixed>> $callback
     * @throws HttpExceptionInterface
     */
    private function withinLock(Transfer $transfer, callable $callback): void
    {
        try {
            Cache::lock("filebeam:turbo:{$transfer->id}:sessions:lock", 5)->block(3, function () use ($transfer, $callback): void {
                /** @var list<array<string, mixed>> $cachedSessions */
                $cachedSessions = Cache::get($this->key($transfer), []);
                $sessions = $this->activeSessions($callback($this->activeSessions($cachedSessions)));
                if ($sessions === []) {
                    Cache::forget($this->key($transfer));

                    return;
                }
                /** @var list<int> $expiryTimes */
                $expiryTimes = array_column($sessions, 'expires_at');
                if ($expiryTimes === []) {
                    Cache::forget($this->key($transfer));

                    return;
                }
                $latestExpiry = max($expiryTimes);
                $cacheExpiry = min($latestExpiry, $transfer->expires_at->getTimestamp());
                Cache::put($this->key($transfer), $sessions, max(1, $cacheExpiry - now()->getTimestamp()));
            });
        } catch (HttpExceptionInterface $exception) {
            throw $exception;
        } catch (Throwable) {
            abort(503, 'Download session reporting is temporarily unavailable.');
        }
    }

    private function key(Transfer $transfer): string
    {
        return "filebeam:turbo:{$transfer->id}:sessions";
    }

    private function sessionExpiry(Transfer $transfer, bool $terminal): int
    {
        $minutes = $terminal
            ? (int) config('filebeam.transfers.session_terminal_minutes')
            : (int) config('filebeam.transfers.session_idle_minutes');

        return min(now()->addMinutes($minutes)->getTimestamp(), $transfer->expires_at->getTimestamp());
    }

    private function nextNumber(Transfer $transfer): int
    {
        $key = "filebeam:turbo:{$transfer->id}:session-number";
        $number = (int) Cache::get($key, 0) + 1;
        Cache::put($key, $number, max(1, $transfer->expires_at->getTimestamp() - now()->getTimestamp()));

        return $number;
    }

    /**
     * @throws HttpExceptionInterface
     */
    public function update(UpdateDownloadSessionRequest $request, Transfer $transfer, string $session): JsonResponse
    {
        abort_unless($transfer->isPublishedTurbo() && $transfer->expires_at->isFuture(), 404);
        /** @var array{sequence: int, progress: float|int, status: string} $validated */
        $validated = $request->validated();
        $token = $request->header('X-Filebeam-Session-Token');

        $this->withinLock($transfer, function (array $sessions) use ($transfer, $session, $token, $validated): array {
            foreach ($sessions as $index => $stored) {
                if ($stored['id'] !== $session) {
                    continue;
                }
                abort_unless(Capability::matches($stored['token_hash'], $token), 403);
                if ($validated['sequence'] <= $stored['sequence']) {
                    return $sessions;
                }
                if (in_array($stored['status'], ['completed', 'cancelled', 'error'], true)) {
                    return $sessions;
                }
                abort_unless($validated['status'] !== 'completed' || $transfer->status === TransferStatus::Available, 409, 'The transfer is not complete.');
                $terminal = in_array($validated['status'], ['completed', 'cancelled', 'error'], true);
                $sessions[$index] = [
                    ...$stored,
                    'sequence' => $validated['sequence'],
                    'progress' => max($stored['progress'], $validated['status'] === 'completed' ? 100 : min(99, $validated['progress'])),
                    'status' => $validated['status'],
                    'heartbeat_at' => now()->getTimestamp(),
                    'expires_at' => $this->sessionExpiry($transfer, $terminal),
                ];

                return $sessions;
            }
            abort(404);
        });

        return response()->json(status: 204);
    }

    /**
     * @param  list<array<string, mixed>>  $sessions
     * @return list<array<string, mixed>>
     */
    private function activeSessions(array $sessions): array
    {
        return array_values(array_filter($sessions, fn (array $session): bool => $session['expires_at'] > now()->getTimestamp()));
    }
}
