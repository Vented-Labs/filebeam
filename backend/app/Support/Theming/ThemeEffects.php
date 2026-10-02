<?php

declare(strict_types=1);

namespace App\Support\Theming;

/** Decorative recipes are independent of text, controls, syntax and structural elevation. */
final class ThemeEffects
{
    private const DARK = [
        'ambient-main' => '#573182', 'ambient-secondary' => '#342043', 'wash' => '#a679ff',
        'note-backing' => '#211b2b', 'emblem-backing' => '#342740', 'card-back' => '#2e2639',
        'card-front-start' => '#3d2c4c', 'card-front-end' => '#2c2138',
        'og' => '#8b35ff', 'orbit' => '#c75bfa',
    ];

    private const LIGHT = [
        'purple' => ['#8b5cf6', '#c66bd4'], 'blue' => ['#4d87d8', '#51a4be'],
        'teal' => ['#309f94', '#70b98d'], 'green' => ['#61a76f', '#93b660'],
        'amber' => ['#d7a443', '#d48b5a'], 'orange' => ['#e59a66', '#d77d93'], 'rose' => ['#d882a0', '#b88acd'],
    ];

    /** Columns: ambient A/B, dropzone, note, emblem, OG glow/shadow/orbit. */
    private const STRENGTHS = [
        'normal' => [0.16, 0.08, 0.16, 0.10, 0.20, 0.22, 0.16, 0.18],
        'amber' => [0.22, 0.10, 0.22, 0.12, 0.22, 0.28, 0.18, 0.22],
        'orange' => [0.19, 0.09, 0.19, 0.11, 0.21, 0.25, 0.17, 0.20],
    ];

    /** @param array<string, string> $surfaces
     * @return array<string, string>
     */
    public static function tokens(string $primary, string $mode, array $surfaces): array
    {
        return ThemeDefinition::mode($mode) === 'dark' ? self::dark($primary) : self::light($primary, $surfaces);
    }

    /** @return array<string, string> */
    private static function dark(string $primary): array
    {
        [$seedL, $seedC, $seedH] = Color::oklch($primary);
        [, $anchorC, $anchorH] = Color::oklch(Palette::DEFAULT_PRIMARY);
        $colors = [];
        foreach (self::DARK as $role => $source) {
            if ($primary === Palette::DEFAULT_PRIMARY) {
                $colors[$role] = $source;
            } elseif ($role === 'og' && $seedL >= 0.45 && $seedL <= 0.85) {
                $colors[$role] = $primary;
            } else {
                [$l, $c, $h] = Color::oklch($source);
                $colors[$role] = Color::hex($l, $seedC < 0.01 ? 0 : $c * min(1.4, $seedC / $anchorC), $seedC < 0.01 ? 0 : fmod($h + $seedH - $anchorH + 360, 360));
            }
        }
        $main = $colors['ambient-main'];
        $secondary = $colors['ambient-secondary'];
        $wash = $colors['wash'];
        $note = $colors['note-backing'];
        $emblem = $colors['emblem-backing'];
        $back = $colors['card-back'];
        $frontStart = $colors['card-front-start'];
        $frontEnd = $colors['card-front-end'];
        $og = implode(' ', Color::channels($colors['og']));
        $orbit = implode(' ', Color::channels($colors['orbit']));

        return [
            '--fb-ambient-glow' => "radial-gradient(ellipse at 49% 12%, {$main}1f, transparent 57%), radial-gradient(ellipse at 90% 39%, {$secondary}13, transparent 45%)",
            '--fb-dropzone-glow' => "radial-gradient(ellipse at 50% 24%, {$wash}13, transparent 55%)",
            '--fb-note-surface' => "radial-gradient(ellipse at 30% 0%, {$wash}12, transparent 72%), $note",
            '--fb-share-mark-surface' => "radial-gradient(circle at 30% 0%, {$wash}28, $emblem)",
            '--fb-dropzone-mark' => "linear-gradient(145deg, $back, $note)",
            '--fb-dropzone-mark-hover' => "linear-gradient(140deg, $frontStart, $frontEnd)",
            '--fb-og-glow' => "rgb($og / 24%)", '--fb-og-shadow' => "rgb($og / 30%)", '--fb-og-orbit' => "rgb($orbit / 12%)",
        ];
    }

    /** @param array<string, string> $surfaces
     * @return array<string, string>
     */
    private static function light(string $primary, array $surfaces): array
    {
        $preset = array_search($primary, ThemeDefinition::SEEDS, true);
        if ($preset !== false) {
            [$a, $b] = self::LIGHT[$preset];
            $strengths = self::STRENGTHS[in_array($preset, ['amber', 'orange'], true) ? $preset : 'normal'];
        } else {
            [, $c, $h] = Color::oklch($primary);
            $a = Color::hex(0.67, $c < 0.01 ? 0 : min($c, 0.13), $c < 0.01 ? 0 : $h);
            $b = Color::hex(0.73, $c < 0.01 ? 0 : min($c, 0.10), $c < 0.01 ? 0 : $h);
            $strengths = self::STRENGTHS[$c >= 0.04 && $h >= 45 && $h <= 105 ? 'amber' : 'normal'];
        }
        [$ambientA, $ambientB, $dropzone, $note, $emblem, $glow, $shadow, $orbit] = $strengths;
        $surface = $surfaces['--fb-surface'];
        $backStart = Color::mixOKLab($a, $surface, 0.04);
        $backEnd = Color::mixOKLab($a, $surface, 0.12);
        $frontStart = Color::mixOKLab($a, $surface, 0.09);
        $frontEnd = Color::mixOKLab($b, $surface, 0.19);

        return [
            '--fb-ambient-glow' => self::radial('ellipse at 49% 12%', $a, $ambientA, 0.4, 24, 57).', '.self::radial('ellipse at 90% 39%', $b, $ambientB, 0.35, 20, 45),
            '--fb-dropzone-glow' => self::radial('ellipse at 50% 24%', $a, $dropzone, 0.4, 24, 55),
            '--fb-note-surface' => self::radial('ellipse at 30% 0%', $a, $note, 0.4, 32, 72).', '.$surfaces['--fb-settings-surface'],
            '--fb-share-mark-surface' => self::radial('circle at 30% 0%', $b, $emblem, 0.3, 42, 100).', '.$surfaces['--fb-accent-surface'],
            '--fb-dropzone-mark' => "linear-gradient(145deg, $backStart, $backEnd)",
            '--fb-dropzone-mark-hover' => "linear-gradient(140deg, $frontStart, $frontEnd)",
            '--fb-og-glow' => self::rgba($a, $glow), '--fb-og-shadow' => self::rgba($a, $shadow), '--fb-og-orbit' => self::rgba($b, $orbit),
        ];
    }

    private static function radial(string $shape, string $source, float $alpha, float $middleWeight, int $middle, int $end): string
    {
        return 'radial-gradient('.$shape.', '.self::rgba($source, $alpha).' 0%, '.self::rgba($source, $alpha * $middleWeight)." $middle%, ".self::rgba($source, 0)." $end%)";
    }

    private static function rgba(string $color, float $alpha): string
    {
        return 'rgb('.implode(' ', Color::channels($color)).' / '.rtrim(rtrim(sprintf('%.4F', $alpha), '0'), '.').')';
    }
}
