<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Actions\Transfers\RemoveTransfer;
use App\Enums\TransferDriver;
use App\Enums\TransferKind;
use App\Enums\TransferRemovalReason;
use App\Enums\TransferStatus;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\User;
use App\Support\EffectivePlan;
use App\Support\TransferHistory;
use Carbon\CarbonImmutable;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Gate;
use Illuminate\Validation\ValidationException;
use Inertia\Inertia;
use Inertia\Response;

class TransferHistoryController extends Controller
{
    public function index(Request $request, TransferHistory $history): JsonResponse|Response
    {
        $user = $request->user();
        assert($user instanceof User);
        $result = $history->listing($request, $user);

        return $request->is('api/*') || $request->expectsJson()
            ? response()->json($result, 200, ['Cache-Control' => 'no-store, private'])
            : Inertia::render('History', ['history' => $result]);
    }

    public function destroy(Request $request, string $id, RemoveTransfer $remove): JsonResponse
    {
        DB::transaction(function () use ($request, $id, $remove): void {
            $transfer = Transfer::query()->ownedBy($request->user()->id)->lockForUpdate()->find($id);
            if ($transfer === null) {
                abort_unless(DB::table('transfer_history_entries')->where('owner_id', $request->user()->id)->where('id', $id)->where('purge_at', '>', now())->exists(), 404);

                return;
            }
            Gate::authorize('manageHistory', $transfer);
            $remove->handle($transfer, TransferRemovalReason::Deleted);
        });

        return response()->json(status: 202, headers: ['Cache-Control' => 'no-store, private']);
    }

    public function extend(Request $request, string $id, EffectivePlan $plans, TransferHistory $history): JsonResponse
    {
        $data = $request->validate(['retention_hours' => ['required', 'integer', 'min:1', 'max:876000']]);
        $transfer = DB::transaction(function () use ($request, $id, $data, $plans, $history): Transfer {
            $user = User::query()->lockForUpdate()->findOrFail($request->user()->id);
            $plan = Plan::query()->lockForUpdate()->findOrFail($plans->resolve($user)->id);
            $transfer = Transfer::query()->ownedBy($user->id)->lockForUpdate()->findOrFail($id);
            Gate::authorize('manageHistory', $transfer);
            abort_unless($transfer->expires_at->isFuture() && (($transfer->driver === TransferDriver::Http && $transfer->status === TransferStatus::Available && $transfer->completed_at !== null) || ($transfer->driver === TransferDriver::WebRtc && $transfer->status === TransferStatus::Live)), 409, 'This transfer cannot be extended.');
            $maximumHours = $transfer->kind === TransferKind::Note ? $plan->maximum_note_retention_hours : $plan->maximum_file_retention_hours;
            if ($data['retention_hours'] > $maximumHours) {
                throw ValidationException::withMessages(['retention_hours' => 'The total retention exceeds your current plan.']);
            }
            $base = $transfer->driver === TransferDriver::WebRtc ? ($transfer->published_at ?? $transfer->created_at) : $transfer->completed_at;
            assert($base !== null);
            $expiry = CarbonImmutable::instance($base)->addHours($data['retention_hours'])->min($history->maximumExpiry($transfer, $plan));
            if ($expiry->equalTo($transfer->expires_at)) {
                return $transfer;
            }
            abort_unless($expiry->greaterThan($transfer->expires_at), 409, 'Retention must move expiry forward.');
            $transfer->update(['retention_hours' => $data['retention_hours'], 'expires_at' => $expiry]);

            return $transfer;
        }, attempts: 3);

        return response()->json(['data' => ['id' => $transfer->id, 'retention_hours' => $transfer->retention_hours, 'expires_at' => $transfer->expires_at->toIso8601String()]], 200, ['Cache-Control' => 'no-store, private']);
    }
}
