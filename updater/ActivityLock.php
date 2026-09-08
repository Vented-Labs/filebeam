<?php

declare(strict_types=1);

namespace Filebeam\Updater;

use RuntimeException;

final class ActivityLock
{
    public function __construct(private readonly string $path) {}

    /**
     * @return resource
     */
    public function acquireShared()
    {
        $handle = $this->open();

        if (! flock($handle, LOCK_SH | LOCK_NB)) {
            fclose($handle);

            throw new RuntimeException('An update is in progress.');
        }

        return $handle;
    }

    /**
     * @return resource
     */
    public function acquireExclusive(int $timeoutSeconds = 120)
    {
        if ($timeoutSeconds < 0) {
            throw new RuntimeException('The update activity lock timeout must not be negative.');
        }

        $handle = $this->open();
        $deadline = hrtime(true) + ($timeoutSeconds * 1_000_000_000);

        do {
            if (flock($handle, LOCK_EX | LOCK_NB)) {
                return $handle;
            }

            usleep(50_000);
        } while (hrtime(true) < $deadline);

        fclose($handle);

        throw new RuntimeException('Timed out waiting for application activity to finish.');
    }

    /**
     * @param  resource  $handle
     */
    public function release($handle): void
    {
        flock($handle, LOCK_UN);
        fclose($handle);
    }

    /**
     * @return resource
     */
    private function open()
    {
        $this->assertNoSymlinks();

        $directory = dirname($this->path);

        if (! is_dir($directory)) {
            if (! mkdir($directory, 0700, true) && ! is_dir($directory)) {
                throw new RuntimeException('Unable to create the update activity lock directory.');
            }

            if (! chmod($directory, 0700)) {
                throw new RuntimeException('Unable to secure the update activity lock directory.');
            }
        }

        $handle = fopen($this->path, 'c');

        if ($handle === false) {
            throw new RuntimeException('Unable to open the update activity lock.');
        }

        $file = fstat($handle);
        $path = lstat($this->path);

        if (! is_array($file) || ! is_array($path) || $file['dev'] !== $path['dev'] || $file['ino'] !== $path['ino']) {
            fclose($handle);

            throw new RuntimeException('The update activity lock changed while it was being opened.');
        }

        if (! chmod($this->path, 0600)) {
            fclose($handle);

            throw new RuntimeException('Unable to secure the update activity lock.');
        }

        return $handle;
    }

    private function assertNoSymlinks(): void
    {
        $path = $this->path;

        while (true) {
            if (is_link($path)) {
                throw new RuntimeException('The update activity lock path must not contain symlinks.');
            }

            $parent = dirname($path);

            if ($path === $parent) {
                return;
            }

            $path = $parent;
        }
    }
}
