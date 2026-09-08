<?php

declare(strict_types=1);

use App\Http\Controllers\Api\V1\DownloadSessionController;
use App\Http\Controllers\Api\V1\TransferChunkController;
use App\Http\Controllers\Api\V1\TransferController;
use App\Http\Controllers\Api\V1\TurboTransferController;
use App\Http\Middleware\EnsureAnonymousTransferUploadsAreEnabled;
use Illuminate\Support\Facades\Route;

Route::prefix('v1')->group(function (): void {
    Route::post('/transfers', [TransferController::class, 'store'])
        ->middleware('throttle:transfer-creation')
        ->name('api.transfers.store');

    Route::get('/transfers/{transfer}', [TransferController::class, 'show'])
        ->middleware('throttle:transfer-reading')
        ->name('api.transfers.show');
    Route::post('/transfers/{transfer}/complete', [TransferController::class, 'complete'])
        ->middleware(['throttle:transfer-writing', EnsureAnonymousTransferUploadsAreEnabled::class])
        ->name('api.transfers.complete');
    Route::delete('/transfers/{transfer}', [TransferController::class, 'destroy'])
        ->middleware('throttle:transfer-writing')
        ->name('api.transfers.destroy');
    Route::post('/transfers/{transfer}/consume', [TransferController::class, 'consume'])
        ->middleware('throttle:transfer-writing')
        ->name('api.transfers.consume');
    Route::put('/transfers/{transfer}/descriptor', [TurboTransferController::class, 'descriptor'])
        ->middleware('throttle:transfer-writing')
        ->name('api.transfers.descriptor');
    Route::get('/transfers/{transfer}/progress', [TurboTransferController::class, 'progress'])
        ->middleware('throttle:transfer-monitoring')
        ->name('api.transfers.progress');
    Route::patch('/transfers/{transfer}/progress', [TurboTransferController::class, 'heartbeat'])
        ->middleware('throttle:transfer-monitoring')
        ->name('api.transfers.progress.heartbeat');
    Route::get('/transfers/{transfer}/monitor', [TurboTransferController::class, 'monitor'])
        ->middleware('throttle:transfer-monitoring')
        ->name('api.transfers.monitor');
    Route::post('/transfers/{transfer}/download-sessions', [DownloadSessionController::class, 'store'])
        ->middleware('throttle:download-session-registration')
        ->name('api.transfers.download-sessions.store');
    Route::patch('/transfers/{transfer}/download-sessions/{session}', [DownloadSessionController::class, 'update'])
        ->middleware('throttle:download-session-reporting')
        ->name('api.transfers.download-sessions.update');

    Route::put('/transfers/{transfer}/items/{item}/chunks/{position}', [TransferChunkController::class, 'store'])
        ->whereNumber('position')
        ->middleware(['throttle:transfer-writing', EnsureAnonymousTransferUploadsAreEnabled::class])
        ->scopeBindings()
        ->name('api.transfer-chunks.store');
    Route::get('/transfers/{transfer}/items/{item}/chunks/{position}', [TransferChunkController::class, 'show'])
        ->whereNumber('position')
        ->middleware('throttle:transfer-reading')
        ->scopeBindings()
        ->name('api.transfer-chunks.show');
});
