<?php

declare(strict_types=1);

namespace App\Support\Theming;

use RuntimeException;

final class TokenContract
{
    /** @param array<string, string> $tokens
     * @param  array<string, string>  $aliases
     * @return array<string, string>
     */
    public static function resolve(array $tokens, array $aliases): array
    {
        $visiting = [];
        $resolve = function (string $name) use (&$resolve, &$tokens, $aliases, &$visiting): string {
            if (isset($visiting[$name])) {
                throw new RuntimeException('Theme alias cycle at '.$name);
            }
            if (isset($tokens[$name])) {
                return $tokens[$name];
            }
            if (! isset($aliases[$name])) {
                throw new RuntimeException('Undefined theme token '.$name);
            }
            $visiting[$name] = true;
            $tokens[$name] = $resolve($aliases[$name]);
            unset($visiting[$name]);

            return $tokens[$name];
        };
        foreach (array_keys($aliases) as $name) {
            $resolve($name);
        }

        return $tokens;
    }

    /** @param array<string, string> $tokens */
    public static function validate(array $tokens, string $context): void
    {
        $types = ThemeDefinition::types();
        if (array_diff_key($types, $tokens) !== [] || array_diff_key($tokens, $types) !== []) {
            throw new RuntimeException($context.': incomplete theme token set');
        }
        foreach ($types as $name => $type) {
            $value = $tokens[$name];
            if ($type === 'color') {
                Color::parse($value);
            } elseif ($type === 'background' && $value !== 'none' && ! str_starts_with($value, 'linear-gradient(')) {
                Color::parse($value);
            }
            if (preg_match('/(?:NaN|Infinity|[{};])/i', $value)) {
                throw new RuntimeException($context.': invalid token '.$name);
            }
            preg_match_all('/var\((--fb-[\w-]+)\)/', $value, $references);
            foreach ($references[1] as $reference) {
                if (! isset($types[$reference]) || $type !== 'non-color') {
                    throw new RuntimeException($context.': unresolved reference in '.$name);
                }
            }
        }
        foreach (['--fb-bg', '--fb-surface', '--fb-surface-raised', '--fb-surface-sunken', '--fb-surface-hover', '--fb-surface-active', '--fb-accent-surface', '--fb-selected-surface'] as $background) {
            foreach (['--fb-text', '--fb-text-muted', '--fb-text-subtle', '--fb-accent-text'] as $foreground) {
                self::contrast($tokens, $foreground, $background, 4.5, $context);
            }
            foreach (['--fb-control-border', '--fb-focus', '--fb-choice-border'] as $foreground) {
                self::contrast($tokens, $foreground, $background, 3, $context);
            }
        }
        foreach (['--fb-action', '--fb-action-hover', '--fb-action-active'] as $background) {
            self::contrast($tokens, '--fb-on-action', $background, 4.8, $context);
        }
        self::contrast($tokens, '--fb-progress-fill', '--fb-progress-track', 3, $context);
    }

    /** @param array<string, string> $tokens */
    private static function contrast(array $tokens, string $foreground, string $background, float $minimum, string $context): void
    {
        $ratio = Color::contrast($tokens[$foreground], $tokens[$background]);
        if ($ratio < $minimum) {
            throw new RuntimeException("$context: $foreground ({$tokens[$foreground]}) on $background ({$tokens[$background]}) is $ratio:1; requires $minimum:1");
        }
    }
}
