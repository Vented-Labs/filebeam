<?php

declare(strict_types=1);

use App\Http\Controllers\InstallationController;
use App\Http\Middleware\InstallationAccess;
use Illuminate\Support\Facades\Route;
use Inertia\Middleware;

Route::prefix('install')->name('install.')->middleware(InstallationAccess::class)->group(function (): void {
    Route::get('/', [InstallationController::class, 'show'])->middleware(Middleware::class)->name('show');
    Route::post('/bootstrap', [InstallationController::class, 'bootstrap'])->name('bootstrap');
    Route::post('/configuration', [InstallationController::class, 'configuration'])->name('configuration');
    Route::post('/database', [InstallationController::class, 'database'])->name('database');
    Route::post('/cache', [InstallationController::class, 'cache'])->name('cache');
    Route::post('/storage', [InstallationController::class, 'storage'])->name('storage');
    Route::put('/probe', [InstallationController::class, 'probe'])->name('probe');
    Route::post('/complete', [InstallationController::class, 'complete'])->name('complete');
});
