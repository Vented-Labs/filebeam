<?php

declare(strict_types=1);

namespace App\Support\Installation;

use Dotenv\Parser\Parser;
use Illuminate\Support\Facades\File;
use InvalidArgumentException;

class EnvironmentWriter
{
    /** @param array<string, string|null> $values */
    public function write(array $values, bool $createOnly = false): void
    {
        if (array_diff(array_keys($values), EnvironmentSettings::outputKeys()) !== []) {
            throw new InvalidArgumentException('Unsupported environment setting.');
        }
        foreach ($values as $value) {
            if (is_string($value) && (str_contains($value, "\0") || str_contains($value, "\r"))) {
                throw new InvalidArgumentException('Environment values contain unsupported characters.');
            }
        }
        $path = config('installation.environment_path') ?: app()->environmentFilePath();
        if (! is_string($path) || $path === '') {
            throw new InvalidArgumentException('The environment file path is invalid.');
        }
        if ($this->hasSymlinkComponent($path) || (! is_file($path) && file_exists($path)) || ($createOnly && File::exists($path))) {
            throw new InvalidArgumentException('The environment file already exists.');
        }
        $lines = File::exists($path) ? preg_split('/\n/', File::get($path)) : [];
        if (! is_array($lines)) {
            throw new InvalidArgumentException('The environment file could not be read.');
        }
        foreach ($lines as $index => $line) {
            if (preg_match('/^\s*(?:export\s+)?([A-Z][A-Z0-9_]*)\s*=/', $line, $matches) !== 1 || ! array_key_exists($matches[1], $values)) {
                continue;
            }
            $key = $matches[1];
            $value = $values[$key];
            if ($value === null) {
                unset($lines[$index]);
            } elseif (! ($key === 'APP_KEY' && trim($line) !== 'APP_KEY=')) {
                $lines[$index] = $key.'='.$this->serialize($value);
            }
        }
        foreach ($values as $key => $value) {
            if ($value !== null && ! $this->hasKey($lines, $key)) {
                $lines[] = $key.'='.$this->serialize($value);
            }
        }

        File::ensureDirectoryExists(dirname($path), 0700, true);
        $temporaryPath = $path.'.'.bin2hex(random_bytes(8)).'.tmp';
        $contents = implode("\n", $lines)."\n";
        (new Parser)->parse($contents);
        $handle = fopen($temporaryPath, 'x');
        if ($handle === false) {
            throw new InvalidArgumentException('The environment file could not be written.');
        }
        try {
            if (! chmod($temporaryPath, 0600) || fwrite($handle, $contents) !== strlen($contents) || ! fflush($handle) || ! fsync($handle)) {
                throw new InvalidArgumentException('The environment file could not be written.');
            }
            if (! ($createOnly ? link($temporaryPath, $path) : rename($temporaryPath, $path))) {
                throw new InvalidArgumentException('The environment file could not be written.');
            }
            chmod($path, 0600);
        } finally {
            fclose($handle);
            if (file_exists($temporaryPath)) {
                @unlink($temporaryPath);
            }
        }
    }

    /** @param array<int, string> $lines */
    private function hasKey(array $lines, string $key): bool
    {
        foreach ($lines as $line) {
            if (preg_match('/^\s*(?:export\s+)?'.preg_quote($key, '/').'\s*=/', $line) === 1) {
                return true;
            }
        }

        return false;
    }

    private function serialize(string $value): string
    {
        return '"'.strtr($value, ['\\' => '\\\\', '"' => '\\"', '$' => '\\$', "\n" => '\\n']).'"';
    }

    private function hasSymlinkComponent(string $path): bool
    {
        $directory = dirname($path);
        while ($directory !== dirname($directory)) {
            if (is_link($directory)) {
                return true;
            }
            $directory = dirname($directory);
        }

        return is_link($directory) || is_link($path);
    }
}
