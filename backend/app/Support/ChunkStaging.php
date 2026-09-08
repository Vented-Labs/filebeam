<?php

declare(strict_types=1);

namespace App\Support;

use App\Models\TransferChunkStage;
use Closure;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\File;
use RuntimeException;

class ChunkStaging
{
    /** @return array{version: int, part_min_bytes: int, part_max_bytes: int, request_target_ms: int, request_budget_ms: int} */
    public function transport(int $chunkBytes): array
    {
        return [
            'version' => 1,
            'part_min_bytes' => min((int) config('filebeam.staging.part_min_bytes'), $chunkBytes + 16),
            'part_max_bytes' => min((int) config('filebeam.staging.part_max_bytes'), $chunkBytes + 16),
            'request_target_ms' => (int) config('filebeam.staging.request_target_ms'),
            'request_budget_ms' => (int) config('filebeam.staging.request_budget_ms'),
        ];
    }

    public function path(TransferChunkStage $stage): string
    {
        return rtrim((string) config('filebeam.staging.root'), '/').'/'.$stage->id.'.bin';
    }

    private function lockPath(TransferChunkStage $stage): string
    {
        return rtrim((string) config('filebeam.staging.root'), '/').'/'.$stage->id.'.lock';
    }

    public function exists(TransferChunkStage $stage): bool
    {
        return is_file($this->path($stage));
    }

    public function create(TransferChunkStage $stage): void
    {
        $this->ensureRoot();
        $handle = @fopen($this->path($stage), 'x+b');
        if (! is_resource($handle)) {
            throw new RuntimeException('Staging storage is unavailable.');
        }
        @chmod($this->path($stage), 0600);
        fclose($handle);
    }

    /** @return resource */
    public function open(TransferChunkStage $stage)
    {
        $handle = @fopen($this->path($stage), 'rb+');
        if (! is_resource($handle)) {
            throw new RuntimeException('Staging storage is unavailable.');
        }

        return $handle;
    }

    /** @return resource */
    public function lock(TransferChunkStage $stage)
    {
        $this->ensureRoot();
        $handle = @fopen($this->lockPath($stage), 'c+b');
        if (! is_resource($handle)) {
            throw new RuntimeException('Staging storage is unavailable.');
        }
        @chmod($this->lockPath($stage), 0600);

        return $handle;
    }

    /** @template T
     * @param  Closure(): T  $callback
     * @return T
     */
    public function capacityLock(Closure $callback): mixed
    {
        $this->ensureRoot();
        $handle = @fopen(rtrim((string) config('filebeam.staging.root'), '/').'/.capacity.lock', 'c+b');
        if (! is_resource($handle)) {
            throw new RuntimeException('Staging storage is unavailable.');
        }
        try {
            if (! flock($handle, LOCK_EX | LOCK_NB)) {
                throw new RuntimeException('Staging storage is unavailable.');
            }

            return $callback();
        } finally {
            flock($handle, LOCK_UN);
            fclose($handle);
        }
    }

    public function remove(TransferChunkStage $stage, bool $locked = false): void
    {
        $handle = $locked ? null : $this->lock($stage);
        try {
            if (! $locked && ! flock($handle, LOCK_EX | LOCK_NB)) {
                throw new RuntimeException('Staging data is active.');
            }
            $path = $this->path($stage);
            if (is_file($path) && ! @unlink($path)) {
                throw new RuntimeException('Unable to remove staging data.');
            }
        } finally {
            if (is_resource($handle)) {
                flock($handle, LOCK_UN);
                fclose($handle);
            }
        }
    }

    public function removeTransfer(string $transferId): bool
    {
        $stages = TransferChunkStage::query()->where('transfer_id', $transferId)->get();
        foreach ($stages as $stage) {
            $lock = $this->lock($stage);
            try {
                if (! flock($lock, LOCK_EX | LOCK_NB)) {
                    return false;
                }
                DB::transaction(function () use ($stage): void {
                    $locked = TransferChunkStage::query()->lockForUpdate()->find($stage->id);
                    if ($locked === null) {
                        return;
                    }
                    $this->remove($locked, locked: true);
                    $locked->delete();
                });
            } catch (RuntimeException) {
                return false;
            } finally {
                flock($lock, LOCK_UN);
                fclose($lock);
            }
        }

        return true;
    }

    public function prune(): void
    {
        foreach (TransferChunkStage::query()->where('expires_at', '<=', now())->get() as $stage) {
            $lock = $this->lock($stage);
            try {
                if (! flock($lock, LOCK_EX | LOCK_NB)) {
                    continue;
                }
                DB::transaction(function () use ($stage): void {
                    $locked = TransferChunkStage::query()->lockForUpdate()->find($stage->id);
                    // A writer may have extended the lease after this row was selected.
                    if ($locked === null || $locked->expires_at->isFuture()) {
                        return;
                    }
                    $this->remove($locked, locked: true);
                    $locked->delete();
                });
            } catch (RuntimeException) {
                continue;
            } finally {
                flock($lock, LOCK_UN);
                fclose($lock);
            }
        }

        // Only sweep our flat, UUID-named private root. Lock sidecars are safe to remove
        // only after their row is absent; stale handles cannot recreate a database row.
        if (! is_dir((string) config('filebeam.staging.root'))) {
            return;
        }
        foreach (File::files((string) config('filebeam.staging.root')) as $file) {
            if (! preg_match('/^[0-9a-f-]{36}\.(?:bin|lock)$/i', $file->getFilename()) || $file->getMTime() > now()->subHour()->getTimestamp()) {
                continue;
            }
            $id = $file->getBasename('.'.$file->getExtension());
            $stage = new TransferChunkStage(['id' => $id]);
            $lock = $this->lock($stage);
            try {
                if (flock($lock, LOCK_EX | LOCK_NB)) {
                    DB::transaction(function () use ($id, $file, $stage): void {
                        if (TransferChunkStage::query()->lockForUpdate()->find($id) === null) {
                            @unlink($file->getPathname());
                            // Lock sidecars are bounded by the retention sweep once their DB row is gone.
                            @unlink($this->lockPath($stage));
                        }
                    });
                }
            } finally {
                flock($lock, LOCK_UN);
                fclose($lock);
            }
        }
    }

    private function ensureRoot(): void
    {
        $root = rtrim((string) config('filebeam.staging.root'), '/');
        if ($root === '' || ! str_starts_with($root, '/') || $root === '/' || str_starts_with($root.'/', rtrim(public_path(), '/').'/')) {
            throw new RuntimeException('Staging storage is unavailable.');
        }
        $existing = is_dir($root);
        if (! $existing) {
            if (! @mkdir($root, 0700, true)) {
                throw new RuntimeException('Staging storage is unavailable.');
            }
        }
        if (($existing && ((fileperms($root) ?: 0) & 0077) !== 0) || ! is_dir($root)) {
            throw new RuntimeException('Staging storage is unavailable.');
        }
    }
}
