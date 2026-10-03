<?php

declare(strict_types=1);

namespace App\Support;

use App\Enums\TransferDriver;
use App\Enums\TransferKind;
use App\Enums\TransferRemovalReason;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\User;
use Carbon\CarbonImmutable;
use Illuminate\Http\Request;
use Illuminate\Pagination\Cursor;
use Illuminate\Support\Facades\DB;
use stdClass;

class TransferHistory
{
    private const COLUMNS = ['id', 'kind', 'delivery', 'driver', 'item_count', 'ciphertext_bytes', 'declared_ciphertext_bytes', 'retention_hours', 'burn_on_read', 'created_at', 'completed_at', 'published_at', 'expires_at'];

    /** @return array{data: list<array<string, mixed>>, next_cursor: string|null} */
    public function listing(Request $request, User $user): array
    {
        $data = $request->validate([
            'limit' => ['sometimes', 'integer', 'min:1', 'max:100'],
            'cursor' => ['nullable', 'string', 'max:1024'],
            'status' => ['nullable', 'in:pending,available,live,ended,deleting,expired,deleted,abandoned,burned,removed'],
            'kind' => ['nullable', 'in:files,note'],
            'driver' => ['nullable', 'in:http,webrtc'],
        ]);
        $cursor = isset($data['cursor']) ? Cursor::fromEncoded($data['cursor']) : null;
        if (isset($data['cursor'])) {
            $parameters = $cursor?->toArray();
            abort_unless(is_array($parameters)
                && is_string($parameters['created_at'] ?? null)
                && preg_match('/\A\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d+)?\z/', $parameters['created_at']) === 1
                && is_string($parameters['id'] ?? null)
                && preg_match('/\A[0-9a-hjkmnp-tv-z]{26}\z/i', $parameters['id']) === 1
                && ($parameters['_pointsToNextItems'] ?? null) === true, 422, 'Invalid history cursor.');
        }
        $live = DB::table('transfers')->where('owner_id', $user->id)->select(self::COLUMNS)
            ->selectRaw("CASE WHEN status <> 'deleting' AND expires_at <= ? THEN 'expired' ELSE status END AS status", [now()])
            ->selectRaw('NULL AS removed_at');
        $removed = DB::table('transfer_history_entries')->where('owner_id', $user->id)->where('purge_at', '>', now())
            ->select([...self::COLUMNS, 'status', 'removed_at']);
        $query = DB::query()->fromSub($live->unionAll($removed), 'history');
        foreach (['status', 'kind', 'driver'] as $filter) {
            if (isset($data[$filter])) {
                $query->where($filter, $data[$filter]);
            }
        }
        $page = $query->orderByDesc('created_at')->orderByDesc('id')->cursorPaginate($data['limit'] ?? 25, ['*'], 'cursor', $cursor);
        $plan = app(EffectivePlan::class)->resolve($user);

        return ['data' => array_values(collect($page->items())->map(fn (stdClass $row): array => $this->resource($row, $plan))->all()), 'next_cursor' => $page->nextCursor()?->encode()];
    }

    public function archive(Transfer $transfer): void
    {
        if ($transfer->owner_id === null) {
            return;
        }
        $summary = $transfer->only([...self::COLUMNS, 'owner_id']);
        foreach (['kind', 'delivery', 'driver'] as $enum) {
            $summary[$enum] = $transfer->$enum->value;
        }
        // Only allowlisted operational metadata survives ciphertext cleanup.
        $summary['status'] = ($transfer->removal_reason ?? TransferRemovalReason::Removed)->value;
        $summary['removed_at'] = now();
        $summary['purge_at'] = now()->addDays(90);
        DB::table('transfer_history_entries')->insert($summary);
    }

    public function maximumExpiry(Transfer $transfer, Plan $plan): CarbonImmutable
    {
        $hours = $transfer->kind === TransferKind::Note ? $plan->maximum_note_retention_hours : $plan->maximum_file_retention_hours;
        $base = $transfer->driver === TransferDriver::WebRtc ? ($transfer->published_at ?? $transfer->created_at) : $transfer->completed_at;
        assert($base !== null);
        $maximum = CarbonImmutable::instance($base)->addHours($hours);
        if ($transfer->driver === TransferDriver::WebRtc) {
            $liveMaximum = CarbonImmutable::instance($transfer->created_at)->addHours((int) config('filebeam.webrtc.live_max_hours'));
            $maximum = $maximum->min($liveMaximum);
        }

        return $maximum;
    }

    /** @return array<string, mixed> */
    private function resource(stdClass $row, Plan $plan): array
    {
        $result = (array) $row;
        foreach (['created_at', 'completed_at', 'published_at', 'expires_at', 'removed_at'] as $date) {
            $result[$date] = $row->$date === null ? null : CarbonImmutable::parse($row->$date)->toIso8601String();
        }
        foreach (['item_count', 'ciphertext_bytes', 'declared_ciphertext_bytes', 'retention_hours'] as $number) {
            $result[$number] = (int) $row->$number;
        }
        $result['burn_on_read'] = (bool) $row->burn_on_read;
        $result['can_delete'] = $row->removed_at === null && $row->status !== 'deleting';
        $result['can_extend'] = false;
        $result['maximum_expires_at'] = null;
        $result['maximum_retention_hours'] = $row->kind === 'note' ? $plan->maximum_note_retention_hours : $plan->maximum_file_retention_hours;
        if ($row->removed_at === null && (($row->driver === 'http' && $row->status === 'available' && $row->completed_at !== null) || ($row->driver === 'webrtc' && $row->status === 'live'))) {
            $transfer = new Transfer;
            $transfer->forceFill((array) $row);
            $maximum = $this->maximumExpiry($transfer, $plan);
            $result['maximum_expires_at'] = $maximum->toIso8601String();
            $result['can_extend'] = $maximum->greaterThan(CarbonImmutable::parse($row->expires_at));
        }

        return $result;
    }
}
