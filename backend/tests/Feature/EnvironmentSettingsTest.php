<?php

declare(strict_types=1);

use App\Support\Installation\EnvironmentSettings;
use Illuminate\Support\Facades\File;

beforeEach(function (): void {
    $this->environmentSettingsDirectory = storage_path('framework/testing/environment-settings-'.bin2hex(random_bytes(8)));
    File::ensureDirectoryExists($this->environmentSettingsDirectory);
    $this->environmentSettingsValues = [];
    foreach (['APP_KEY', 'APP_KEY_FILE', 'DB_PASSWORD', 'DB_PASSWORD_FILE', 'FILEBEAM_DATA_DIR'] as $key) {
        $this->environmentSettingsValues[$key] = getenv($key);
    }
});

afterEach(function (): void {
    foreach ($this->environmentSettingsValues as $key => $value) {
        putenv($key.($value === false ? '' : '='.$value));
        if ($value === false) {
            unset($_ENV[$key], $_SERVER[$key]);
        } else {
            $_ENV[$key] = $value;
            $_SERVER[$key] = $value;
        }
    }
    File::deleteDirectory($this->environmentSettingsDirectory);
});

test('loads allowlisted secret files and accepts Docker secret symlinks', function (): void {
    $secret = $this->environmentSettingsDirectory.'/db-password';
    File::put($secret, "database-secret\n");
    symlink($secret, $this->environmentSettingsDirectory.'/mounted-secret');
    putenv('DB_PASSWORD');
    putenv('DB_PASSWORD_FILE='.$this->environmentSettingsDirectory.'/mounted-secret');

    EnvironmentSettings::resolveFileSecrets();

    expect(getenv('DB_PASSWORD'))->toBe('database-secret');
    expect(getenv('DB_PASSWORD_FILE'))->toBeFalse();
    EnvironmentSettings::resolveFileSecrets();
    expect(getenv('DB_PASSWORD'))->toBe('database-secret');
});

test('rejects direct and file secret values together', function (): void {
    $secret = $this->environmentSettingsDirectory.'/db-password';
    File::put($secret, 'database-secret');
    putenv('DB_PASSWORD=direct-secret');
    putenv('DB_PASSWORD_FILE='.$secret);

    expect(fn (): mixed => EnvironmentSettings::resolveFileSecrets())->toThrow(InvalidArgumentException::class);
});

test('validates custom container data directories before bootstrap uses them', function (): void {
    putenv('FILEBEAM_DATA_DIR=/srv/filebeam');
    expect(EnvironmentSettings::containerDataDirectory())->toBe('/srv/filebeam');

    putenv('FILEBEAM_DATA_DIR=relative');
    expect(fn (): string => EnvironmentSettings::containerDataDirectory())->toThrow(InvalidArgumentException::class);

    putenv('FILEBEAM_DATA_DIR=/srv/filebeam/');
    expect(fn (): string => EnvironmentSettings::containerDataDirectory())->toThrow(InvalidArgumentException::class);

    File::ensureDirectoryExists($this->environmentSettingsDirectory.'/actual-data');
    symlink($this->environmentSettingsDirectory.'/actual-data', $this->environmentSettingsDirectory.'/linked-data');
    putenv('FILEBEAM_DATA_DIR='.$this->environmentSettingsDirectory.'/linked-data');
    expect(fn (): string => EnvironmentSettings::containerDataDirectory())->toThrow(InvalidArgumentException::class);
});
