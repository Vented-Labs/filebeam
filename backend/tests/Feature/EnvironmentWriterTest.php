<?php

declare(strict_types=1);

use App\Support\Installation\EnvironmentWriter;
use Dotenv\Parser\Parser;
use Illuminate\Support\Facades\File;

test('updates all duplicate approved keys and preserves an existing app key', function (): void {
    $path = storage_path('framework/testing/installation.env');
    File::put($path, "UNRELATED=value\nAPP_KEY=existing\nDB_PASSWORD=old\nDB_PASSWORD=older\n");
    config()->set('installation.environment_path', $path);

    app(EnvironmentWriter::class)->write(['DB_PASSWORD' => 'a $value # quoted', 'APP_KEY' => 'new']);

    expect(File::get($path))->toContain("UNRELATED=value\n")->toContain("APP_KEY=existing\n");
    expect(substr_count(File::get($path), 'DB_PASSWORD='))->toBe(2);
    File::delete($path);
});

test('writes dotenv values that round trip special characters without interpolation', function (): void {
    $path = storage_path('framework/testing/installation.env');
    config()->set('installation.environment_path', $path);
    $value = "quote' slash\\ dollar$ hash#\nnext";

    app(EnvironmentWriter::class)->write(['DB_PASSWORD' => $value]);

    $entry = (new Parser)->parse(File::get($path))[0];
    expect($entry->getValue()->get()->getChars())->toBe($value);
    File::delete($path);
});

test('refuses create only writes when the environment file exists', function (): void {
    $path = storage_path('framework/testing/installation.env');
    File::put($path, "APP_NAME=existing\n");
    config()->set('installation.environment_path', $path);

    expect(fn (): mixed => app(EnvironmentWriter::class)->write(['APP_NAME' => 'New'], true))->toThrow(InvalidArgumentException::class);

    File::delete($path);
});

test('rejects unsupported keys and unsafe line endings', function (): void {
    expect(fn (): mixed => app(EnvironmentWriter::class)->write(['UNRELATED' => 'value']))->toThrow(InvalidArgumentException::class);
    expect(fn (): mixed => app(EnvironmentWriter::class)->write(['APP_NAME' => "bad\r\nvalue"]))->toThrow(InvalidArgumentException::class);
});

test('accepts the username routing deployment override key', function (): void {
    $path = storage_path('framework/testing/installation.env');
    config()->set('installation.environment_path', $path);

    app(EnvironmentWriter::class)->write(['FILEBEAM_USERNAME_ROUTING_ENABLED' => 'false']);

    expect(File::get($path))->toContain('FILEBEAM_USERNAME_ROUTING_ENABLED="false"');
    File::delete($path);
});

test('refuses an environment path below a symbolic link', function (): void {
    $root = storage_path('framework/testing/environment-writer-'.bin2hex(random_bytes(8)));
    $originalPath = config('installation.environment_path');
    File::ensureDirectoryExists($root.'/outside');
    symlink($root.'/outside', $root.'/config');
    config()->set('installation.environment_path', $root.'/config/.env');

    try {
        expect(fn (): mixed => app(EnvironmentWriter::class)->write(['APP_NAME' => 'Filebeam']))->toThrow(InvalidArgumentException::class);
    } finally {
        config()->set('installation.environment_path', $originalPath);
    }

    File::deleteDirectory($root);
});
