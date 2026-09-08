<?php

declare(strict_types=1);

use App\Support\Installation\ChunkSize;

it('parses PHP ini size limits', function (string $value, ?int $expected): void {
    expect(ChunkSize::parseLimit($value))->toBe($expected);
})->with([
    'bytes' => ['123', 123],
    'kilobytes' => ['8K', 8_192],
    'megabytes' => ['25m', 26_214_400],
    'gigabytes' => ['1G', 1_073_741_824],
    'unlimited zero' => ['0', null],
    'unlimited negative one' => ['-1', null],
    'overflowing bytes' => ['999999999999999999999999999999', null],
    'malformed suffix' => ['25MB', null],
    'malformed value' => ['not-a-limit', null],
]);

it('recommends an encrypted body below a finite post limit', function (): void {
    $limits = (new ChunkSize)->describe('20M', '2M');

    expect($limits)->toBe([
        'maximum' => 25_000_000,
        'minimum' => 17,
        'recommended' => 20_971_520,
        'post_max_size' => '20M',
        'post_max_bytes' => 20_971_520,
        'upload_max_filesize' => '2M',
        'overhead' => 16,
    ]);
});

it('uses the maximum chunk size when the post limit is unlimited', function (): void {
    $limits = (new ChunkSize)->describe('-1', '2M');

    expect($limits['recommended'])->toBe(25_000_000)
        ->and($limits['post_max_bytes'])->toBeNull();
});

it('uses the minimum chunk size when the post limit is malformed', function (): void {
    $limits = (new ChunkSize)->describe('unknown', '2M');

    expect($limits['recommended'])->toBe(17)
        ->and($limits['post_max_bytes'])->toBeNull();
});

it('warns when a finite post limit cannot contain the selected ciphertext body', function (): void {
    $sizes = new ChunkSize('8M', '2M');

    expect($sizes->warning(8_388_609))->toBeTrue()
        ->and($sizes->warning(8_388_608))->toBeFalse();
});
