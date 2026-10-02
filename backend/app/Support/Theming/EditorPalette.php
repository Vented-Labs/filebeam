<?php

declare(strict_types=1);

namespace App\Support\Theming;

/** Preset-aware editor surfaces and syntax accents, with independent reading contrast. */
final class EditorPalette
{
    /** @param array<string, string> $ui
     * @return array<string, string>
     */
    public static function tokens(string $primary, string $mode, array $ui): array
    {
        $base = ThemeDefinition::fixed($mode);
        if ($primary === Palette::DEFAULT_PRIMARY) {
            return [];
        }
        $light = $mode === 'light';
        $bg = $light ? Color::mixOKLab($ui['--fb-surface-sunken'], '#ffffff', 0.18) : $ui['--fb-surface-sunken'];
        $gutter = $light ? Color::mixOKLab($ui['--fb-surface-sunken'], $bg, 0.75) : $ui['--fb-surface'];
        $selected = $ui['--fb-selected-surface'];
        $matching = Color::mixOKLab($selected, $light ? $bg : $gutter, 0.75);
        $tokens = [
            '--fb-editor-bg' => $bg, '--fb-editor-gutter-bg' => $gutter,
            '--fb-editor-border' => $ui['--fb-border'], '--fb-editor-gutter-active-bg' => $matching,
            '--fb-editor-selection' => $selected,
            '--fb-editor-selection-inactive' => Color::mixOKLab($selected, $bg, 0.6),
            '--fb-editor-matching-bracket' => $matching,
        ];
        $backgrounds = [$bg, $selected, $tokens['--fb-editor-selection-inactive'], $matching, $base['--fb-editor-search-match']];
        $backgrounds = [...$backgrounds, ...array_map(static fn (string $background): string => Color::composite($base['--fb-editor-active-line'], $background), $backgrounds)];
        [, $seedC, $seedH] = Color::oklch($primary);
        [, , $anchorH] = Color::oklch(Palette::DEFAULT_PRIMARY);
        foreach (['--fb-editor-keyword', '--fb-editor-type', '--fb-editor-heading'] as $role) {
            [$l, $c, $h] = Color::oklch($base[$role]);
            $c = $seedC < 0.01 ? 0 : min($c, $seedC * 0.8, 0.14);
            $h = $seedC < 0.01 ? 0 : fmod($h + $seedH - $anchorH + 360, 360);
            $ink = Color::against([$l, $c, $h], $backgrounds, 4.8, $light);
            $tokens[$role] = Color::against(Color::oklch($ink), [$bg], 5.5, $light);
        }
        $tokens['--fb-editor-bracket-border'] = $tokens['--fb-editor-keyword'];

        return $tokens;
    }
}
