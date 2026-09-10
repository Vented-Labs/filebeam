<?php

declare(strict_types=1);

namespace App\Support;

use App\Models\TransferChunk;
use DateTimeImmutable;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\StreamedResponse;

class ChunkResponse
{
    private const int MAX_RANGE_HEADER_BYTES = 128;

    public function __construct(private ChunkReader $reader) {}

    public function make(Request $request, TransferChunk $chunk): StreamedResponse
    {
        $bytes = $chunk->ciphertext_bytes;
        $etag = '"'.$chunk->checksum.'"';
        $headers = [
            'Accept-Ranges' => 'bytes',
            'Cache-Control' => 'private, no-store',
            'Content-Type' => 'application/octet-stream',
            'ETag' => $etag,
            'X-Content-Type-Options' => 'nosniff',
        ];
        $range = $this->range($request, $chunk, $etag);

        if ($range === false) {
            return response()->stream(static function (): void {}, 416, [
                ...$headers,
                'Content-Length' => '0',
                'Content-Range' => "bytes */{$bytes}",
            ]);
        }

        $stream = $this->reader->read($chunk);
        [$start, $end] = $range ?? [0, $bytes - 1];
        $length = $end - $start + 1;

        return response()->stream(function () use ($request, $stream, $start, $length): void {
            try {
                if ($request->isMethod('HEAD')) {
                    return;
                }
                fseek($stream, $start);
                $remaining = $length;
                while ($remaining > 0 && ! feof($stream)) {
                    $part = fread($stream, min(1_048_576, $remaining));
                    if ($part === false) {
                        break;
                    }
                    $remaining -= strlen($part);
                    echo $part;
                }
            } finally {
                fclose($stream);
            }
        }, $range === null ? 200 : 206, [
            ...$headers,
            'Content-Length' => (string) $length,
            ...($range === null ? [] : ['Content-Range' => "bytes {$start}-{$end}/{$bytes}"]),
        ]);
    }

    /** @return array{int, int}|false|null */
    private function range(Request $request, TransferChunk $chunk, string $etag): array|false|null
    {
        $header = $request->header('Range');
        if (! is_string($header) || $header === '') {
            return null;
        }
        if (! $this->ifRangeMatches($request->header('If-Range'), $chunk, $etag)) {
            return null;
        }
        if (strlen($header) > self::MAX_RANGE_HEADER_BYTES || preg_match('/^bytes=(\d*)-(\d*)$/D', $header, $matches) !== 1) {
            return false;
        }

        $bytes = $chunk->ciphertext_bytes;
        $start = $matches[1] === '' ? null : (int) $matches[1];
        $end = $matches[2] === '' ? null : (int) $matches[2];
        if ($start === null) {
            if ($end === null || $end < 1) {
                return false;
            }

            return [max(0, $bytes - $end), $bytes - 1];
        }
        if ($start >= $bytes) {
            return false;
        }
        $end ??= $bytes - 1;
        $end = min($end, $bytes - 1);

        return $end < $start ? false : [$start, $end];
    }

    private function ifRangeMatches(?string $ifRange, TransferChunk $chunk, string $etag): bool
    {
        if ($ifRange === null || $ifRange === '') {
            return true;
        }
        if (strlen($ifRange) > self::MAX_RANGE_HEADER_BYTES) {
            return false;
        }
        if (str_starts_with($ifRange, '"')) {
            return hash_equals($etag, $ifRange);
        }
        $date = DateTimeImmutable::createFromFormat(DateTimeImmutable::RFC7231, $ifRange);

        return $date !== false && $chunk->updated_at->getTimestamp() <= $date->getTimestamp();
    }
}
