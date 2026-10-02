<?php

declare(strict_types=1);

use App\Http\Controllers\ThemeAssetController;
use Illuminate\Support\Facades\Route;

Route::get('/_theme/{version}/{primary}/{mode}/{asset}', ThemeAssetController::class)
    ->where('version', '[0-9a-f]{24}')
    ->where('primary', '[0-9a-f]{6}')
    ->where('mode', 'dark|light')
    ->where('asset', '[a-z0-9-]+\.(svg|png|ico)')
    ->name('theme.asset');
