<?php

declare(strict_types=1);

use App\Http\Controllers\FileReportController;
use Illuminate\Support\Facades\Route;

Route::get('/reports/create', [FileReportController::class, 'create'])->name('reports.create');
Route::post('/reports', [FileReportController::class, 'store'])
    ->middleware('throttle:file-reports')
    ->name('reports.store');
