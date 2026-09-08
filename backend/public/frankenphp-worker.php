<?php

declare(strict_types=1);

$_SERVER['APP_BASE_PATH'] = dirname(__DIR__);
$_SERVER['APP_PUBLIC_PATH'] = __DIR__;

$maxRequests = filter_var($_SERVER['MAX_REQUESTS'] ?? getenv('MAX_REQUESTS') ?: 500, FILTER_VALIDATE_INT);
$maxRequests = $maxRequests !== false && $maxRequests > 0 ? $maxRequests : 500;
$_ENV['MAX_REQUESTS'] = $maxRequests;
$_SERVER['MAX_REQUESTS'] = $maxRequests;

require dirname(__DIR__).'/vendor/laravel/octane/bin/frankenphp-worker.php';
