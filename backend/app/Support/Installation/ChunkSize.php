<?php

declare(strict_types=1);

namespace App\Support\Installation;

class ChunkSize
{
    private const int Overhead = 16;

    private const int Minimum = 17;

    private const int MaximumEncrypted = 25_000_000;

    public function __construct(
        private readonly ?string $postMaxSize = null,
        private readonly ?string $uploadMaxFilesize = null,
    ) {}

    /**
     * @return array{maximum: int, minimum: int, recommended: int, post_max_size: string, post_max_bytes: ?int, upload_max_filesize: string, overhead: int}
     */
    public function limits(): array
    {
        return $this->describe(
            $this->postMaxSize ?? (string) ini_get('post_max_size'),
            $this->uploadMaxFilesize ?? (string) ini_get('upload_max_filesize'),
        );
    }

    /**
     * @return array{maximum: int, minimum: int, recommended: int, post_max_size: string, post_max_bytes: ?int, upload_max_filesize: string, overhead: int}
     */
    public function describe(string $postMaxSize, string $uploadMaxFilesize): array
    {
        $postMaxBytes = self::parseLimit($postMaxSize);
        $maximum = self::MaximumEncrypted;

        if ($postMaxBytes === null) {
            $recommended = self::isUnlimited($postMaxSize) ? $maximum : self::Minimum;
        } else {
            $recommended = max(self::Minimum, min($maximum, $postMaxBytes));
        }

        return [
            'maximum' => $maximum,
            'minimum' => self::Minimum,
            'recommended' => $recommended,
            'post_max_size' => $postMaxSize,
            'post_max_bytes' => $postMaxBytes,
            'upload_max_filesize' => $uploadMaxFilesize,
            'overhead' => self::Overhead,
        ];
    }

    public static function parseLimit(string $value): ?int
    {
        $value = trim($value);

        if (self::isUnlimited($value) || preg_match('/^(\d+)\s*([KMG])?$/i', $value, $matches) !== 1) {
            return null;
        }

        $multipliers = ['K' => 1_024, 'M' => 1_048_576, 'G' => 1_073_741_824];
        $multiplier = $multipliers[strtoupper($matches[2] ?? '')] ?? 1;
        $bytes = ltrim($matches[1], '0');
        $bytes = $bytes === '' ? '0' : $bytes;
        $maximum = (string) intdiv(PHP_INT_MAX, $multiplier);

        if (strlen($bytes) > strlen($maximum) || (strlen($bytes) === strlen($maximum) && $bytes > $maximum)) {
            return null;
        }

        return (int) $bytes * $multiplier;
    }

    public function warning(int $selected): bool
    {
        $postMaxBytes = $this->limits()['post_max_bytes'];

        return $postMaxBytes !== null && $selected > $postMaxBytes;
    }

    private static function isUnlimited(string $value): bool
    {
        return in_array(trim($value), ['0', '-1'], true);
    }
}
