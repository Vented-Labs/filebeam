<?php

declare(strict_types=1);

use App\Http\Controllers\AccountKeyController;
use App\Http\Controllers\HomeController;
use App\Http\Controllers\InboxController;
use App\Http\Controllers\ReceiveController;
use Illuminate\Http\JsonResponse;
use Illuminate\Support\Facades\Route;
use Inertia\Inertia;
use Inertia\Response;

require __DIR__.'/auth.php';

Route::get('/', HomeController::class)->name('home');

Route::get('/updater/probe', function (): JsonResponse {
    $path = rtrim((string) config('filebeam.updates.state_path'), '/').'/probe.json';
    $probe = is_file($path)
        ? json_decode((string) file_get_contents($path), true)
        : null;
    $token = request()->query('token');

    if (! is_array($probe) || ! is_string($token) || ! is_string($probe['token'] ?? null) || ! hash_equals($probe['token'], $token)) {
        abort(403);
    }

    return response()->json([
        'token' => $token,
        'version' => config('version.version'),
        'built_at' => config('version.built_at'),
        ...(config('version.distribution') === 'package' ? ['activity_protocol' => 1] : []),
    ], 200, ['Cache-Control' => 'no-store, private']);
})->name('updater.probe');

Route::middleware('auth')->group(function (): void {
    Route::get('/account/keys', [AccountKeyController::class, 'index'])->name('account.keys.index');
    Route::post('/account/keys', [AccountKeyController::class, 'store'])->middleware('throttle:account-key-writing')->name('account.keys.store');
    Route::patch('/account/keys/{bundle}', [AccountKeyController::class, 'update'])->name('account.keys.update');
    Route::patch('/account/inbox', [InboxController::class, 'update'])->name('account.inbox.update');
    Route::patch('/account/notifications', [InboxController::class, 'notifications'])->name('account.notifications.update');
    Route::get('/account/inbox', [InboxController::class, 'index'])->name('inbox.index');
    Route::get('/account/inbox/{transfer}/items/{item}/chunks/{position}', [InboxController::class, 'chunk'])->whereNumber('position')->scopeBindings()->middleware('throttle:transfer-reading')->name('inbox.chunk');
    Route::get('/account/inbox/{transfer}/metadata', [InboxController::class, 'metadata'])->middleware('throttle:transfer-reading')->name('inbox.metadata');
    Route::get('/account/inbox/{transfer}', [InboxController::class, 'show'])->name('inbox.show');
    Route::delete('/account/inbox/{transfer}', [InboxController::class, 'destroy'])->name('inbox.destroy');
});

Route::get('/u/{username}', ReceiveController::class)->where('username', '[a-z0-9_]{3,24}')->name('receive.show');

if (is_string(config('filebeam.username_domain'))) {
    Route::domain(config('filebeam.username_domain'))->get('/{username}', ReceiveController::class)->where('username', '[a-z0-9_]{3,24}')->name('receive.domain');
}

Route::get('/{transferId}', fn (string $transferId): Response => Inertia::render('Transfer', [
    'transferId' => $transferId,
]))->where('transferId', '[0-7][0-9A-HJKMNP-TV-Z]{25}')->name('transfers.show');
