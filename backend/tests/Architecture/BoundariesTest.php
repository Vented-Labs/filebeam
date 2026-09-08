<?php

declare(strict_types=1);

use Filebeam\Updater\Catalog;
use Filebeam\Updater\Command;
use Filebeam\Updater\Env;
use Filebeam\Updater\Http;
use Filebeam\Updater\Package;
use Filebeam\Updater\Recovery;
use Filebeam\Updater\Semver;
use Filebeam\Updater\State;
use Illuminate\Foundation\Http\FormRequest;
use Illuminate\Http\Resources\Json\JsonResource;
use Symfony\Component\Finder\Finder;

$backend = dirname(__DIR__, 2);
$root = dirname($backend);

require_once $root.'/updater/Updater.php';

arch('production code')
    ->expect('App')
    ->not->toUse(['Tests', 'Pest', 'PHPUnit', 'Mockery', 'dd', 'dump', 'var_dump', 'ray']);

arch('domain and background code')
    ->expect(['App\\Models', 'App\\Support', 'App\\Actions', 'App\\Jobs'])
    ->not->toUse(['App\\Http\\Controllers', 'App\\Filament\\Pages', 'App\\Filament\\Resources']);

arch('form requests')
    ->expect('App\\Http\\Requests')
    ->toExtend(FormRequest::class);

arch('API resources')
    ->expect('App\\Http\\Resources')
    ->toExtend(JsonResource::class);

test('standalone updater classes are loaded and do not depend on Laravel', function () use ($root): void {
    $classes = [
        Command::class,
        State::class,
        Recovery::class,
        Package::class,
        Catalog::class,
        Http::class,
        Env::class,
        Semver::class,
    ];

    expect($classes)->not->toBeEmpty();
    foreach ($classes as $class) {
        expect(class_exists($class))->toBeTrue();
    }

    $dependencies = array_filter(
        token_get_all((string) file_get_contents($root.'/updater/Updater.php')),
        fn (array|string $token): bool => is_array($token)
            && in_array($token[0], [T_NAME_QUALIFIED, T_NAME_FULLY_QUALIFIED], true),
    );

    foreach ($dependencies as $dependency) {
        expect($dependency[1])
            ->not->toStartWith('App\\')
            ->not->toStartWith('Illuminate\\');
    }
});

test('environment reads stay at bootstrap and installation boundaries', function () use ($backend): void {
    $files = Finder::create()
        ->files()
        ->in([$backend.'/app', $backend.'/public'])
        ->name('*.php')
        ->exclude('cache');

    foreach ($files as $file) {
        if (str_starts_with($file->getPathname(), $backend.'/app/Support/Installation/')
            || $file->getPathname() === $backend.'/public/frankenphp-worker.php') {
            continue;
        }

        expect((string) file_get_contents($file->getPathname()))
            ->not->toMatch('/\b(?:env|getenv|putenv)\s*\(|\$_(?:ENV|SERVER)\b/');
    }
});

test('production enums live in the App\\Enums namespace and directory', function () use ($backend): void {
    $enumFiles = [];

    foreach (Finder::create()->files()->in($backend.'/app')->name('*.php') as $file) {
        $source = (string) file_get_contents($file->getPathname());

        $containsEnum = collect(token_get_all($source))
            ->contains(fn (array|string $token): bool => is_array($token) && $token[0] === T_ENUM);

        if ($containsEnum) {
            $enumFiles[] = $file;
        }
    }

    expect($enumFiles)->not->toBeEmpty();

    foreach ($enumFiles as $file) {
        $path = $file->getPathname();
        $source = (string) file_get_contents($path);

        expect($path)->toStartWith($backend.'/app/Enums/')
            ->and($source)->toMatch('/namespace\s+App\\\\Enums\s*;/');
    }
});

test('models use scope attributes instead of legacy scope method names', function () use ($backend): void {
    $models = Finder::create()->files()->in($backend.'/app/Models')->name('*.php');

    foreach ($models as $model) {
        expect((string) file_get_contents($model->getPathname()))
            ->not->toMatch('/function\s+scope[A-Z]\w*\s*\(/');
    }
});
