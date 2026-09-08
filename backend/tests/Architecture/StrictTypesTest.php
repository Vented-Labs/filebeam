<?php

declare(strict_types=1);

use Symfony\Component\Finder\Finder;

test('application, test, updater, and release PHP entrypoints declare strict types', function (): void {
    $backend = dirname(__DIR__, 2);
    $root = dirname($backend);
    $directories = [
        $backend.'/app',
        $backend.'/bootstrap',
        $backend.'/config',
        $backend.'/database',
        $backend.'/routes',
        $backend.'/public',
        $backend.'/tests',
        $root.'/updater',
        $root.'/scripts/release',
        $root.'/tests/updater',
    ];
    $files = iterator_to_array(
        Finder::create()
            ->files()
            ->in($directories)
            ->name('*.php')
            ->exclude('cache'),
    );

    foreach ([$backend.'/artisan', $backend.'/public/index.php', $root.'/update.php'] as $path) {
        $files[] = new SplFileInfo($path);
    }

    expect($files)->not->toBeEmpty();

    foreach ($files as $file) {
        $source = file_get_contents($file->getPathname());
        expect($source)->not->toBeFalse();

        if (str_starts_with($source, '#!')) {
            $source = substr($source, strpos($source, "\n") + 1);
        }

        $tokens = array_values(array_filter(
            token_get_all($source),
            fn (array|string $token): bool => ! is_array($token)
                || ! in_array($token[0], [T_OPEN_TAG, T_WHITESPACE, T_COMMENT, T_DOC_COMMENT], true),
        ));
        $declaration = array_slice($tokens, 0, 7);
        $isStrict = is_array($declaration[0] ?? null)
            && $declaration[0][0] === T_DECLARE
            && ($declaration[1] ?? null) === '('
            && is_array($declaration[2] ?? null)
            && $declaration[2][1] === 'strict_types'
            && ($declaration[3] ?? null) === '='
            && is_array($declaration[4] ?? null)
            && $declaration[4][1] === '1'
            && ($declaration[5] ?? null) === ')'
            && ($declaration[6] ?? null) === ';';

        expect($isStrict)->toBeTrue($file->getPathname().' must declare strict_types=1.');
    }
});
