<?php

declare(strict_types=1);

use Illuminate\Foundation\Console\ClosureCommand;
use Illuminate\Support\Facades\Artisan;
use Tests\TestCase;

uses(TestCase::class);

test('application Artisan commands use the filebeam prefix', function () {
    $consoleRoutes = realpath(base_path('routes/console.php'));
    $applicationCommandNames = [];

    foreach (Artisan::all() as $name => $command) {
        if (str_starts_with($command::class, 'App\\Console\\Commands\\')) {
            $applicationCommandNames[] = $name;

            continue;
        }

        if ($command instanceof ClosureCommand) {
            $callback = (new ReflectionProperty($command, 'callback'))->getValue($command);

            if ((new ReflectionFunction($callback))->getFileName() === $consoleRoutes) {
                $applicationCommandNames[] = $name;
            }
        }
    }

    expect($applicationCommandNames)->not->toBeEmpty();

    foreach ($applicationCommandNames as $name) {
        expect($name)->toStartWith('filebeam:');
    }
});
