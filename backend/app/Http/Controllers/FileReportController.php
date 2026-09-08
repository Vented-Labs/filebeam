<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Http\Requests\StoreFileReportRequest;
use App\Models\FileReport;
use App\Models\Transfer;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\RedirectResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Inertia\Inertia;
use Inertia\Response;
use Throwable;

class FileReportController extends Controller
{
    /**
     * @throws Throwable
     */
    public function store(StoreFileReportRequest $request): JsonResponse|RedirectResponse
    {
        $validated = $request->validated();

        if (($validated['website'] ?? null) === null) {
            DB::transaction(function () use ($request, $validated): void {
                $transfer = Transfer::query()
                    ->lockForUpdate()
                    ->whereKey($validated['transfer_id'])
                    ->availableAndUnexpired()
                    ->first();

                if ($transfer === null) {
                    return;
                }

                FileReport::query()->create([
                    'transfer_id' => $transfer->id,
                    'transfer_identifier' => $transfer->id,
                    'reporter_id' => $request->user('web')?->id,
                    'reporter_email' => $validated['reporter_email'] ?? null,
                    'category' => $validated['category'],
                    'description' => $validated['description'],
                ]);
            });
        }

        if ($request->expectsJson()) {
            return response()->json(['status' => 'received'], 202);
        }

        return to_route('reports.create', [
            'created' => 1,
            'transfer_id' => $validated['transfer_id'],
        ]);
    }

    public function create(Request $request): Response
    {
        $transferId = $request->query('transfer_id');

        return Inertia::render('Reports/Create', [
            'created' => $request->boolean('created'),
            'transferId' => is_string($transferId) && preg_match('/^[0-7][0-9A-HJKMNP-TV-Z]{25}$/i', $transferId) === 1
                ? $transferId
                : null,
        ]);
    }
}
