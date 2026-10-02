<?php

declare(strict_types=1);

namespace App\Support\Theming;

use InvalidArgumentException;

/** Versioned role definitions shared by the web, gallery, mail and artwork adapters. */
final class ThemeDefinition
{
    public const SEEDS = [
        'purple' => '#8b35ff', 'blue' => '#3d78d8', 'teal' => '#168f85',
        'green' => '#388b59', 'amber' => '#cf9b35', 'orange' => '#da7b40', 'rose' => '#cf5d78',
    ];

    public const ENDPOINTS = [
        'purple' => '#c05ada', 'blue' => '#4a9dbd', 'teal' => '#48a778',
        'green' => '#76a543', 'amber' => '#d78135', 'orange' => '#d46369', 'rose' => '#b570c0',
    ];

    private const COMMON = [
        'font-ui' => "'Inter Variable', 'Inter', ui-sans-serif, system-ui, -apple-system, 'Segoe UI', sans-serif",
        'font-brand' => "'Plus Jakarta Sans Variable', 'Plus Jakarta Sans', var(--fb-font-ui)",
        'font-code' => "'JetBrains Mono Variable', 'JetBrains Mono', ui-monospace, 'SFMono-Regular', Consolas, monospace",
        'font-body' => '1.1rem', 'font-small' => '0.9625rem', 'font-code-size' => '0.9625rem',
        'line-body' => '1.5', 'line-code' => '1.65', 'weight-normal' => '400', 'weight-medium' => '500',
        'weight-semibold' => '600', 'weight-bold' => '700', 'weight-brand' => '800',
        'radius-sm' => '0.5rem', 'radius-control' => '0.625rem', 'radius-panel' => '1.5rem', 'space-unit' => '0.25rem',
        'ease' => 'cubic-bezier(0.22, 1, 0.36, 1)', 'ease-hover' => 'cubic-bezier(0.4, 0, 0.2, 1)',
        'duration-control' => '180ms', 'duration-switch' => '300ms', 'duration-selection' => '380ms',
        'duration-menu-in' => '280ms', 'duration-menu-out' => '200ms', 'duration-pane' => '460ms',
        'duration-dialog-in' => '360ms', 'duration-dialog-out' => '210ms', 'duration-dialog-content' => '400ms',
        'duration-toast-in' => '280ms', 'duration-toast-out' => '180ms', 'duration-fast' => '180ms', 'duration-normal' => '200ms',
    ];

    /** Columns: browser chrome, page, surface, raised, sunken, footer, settings, border, control, control hover, hover, active. */
    private const NEUTRALS = [
        'light' => [
            'purple' => 'f7f7fa f7f7fa fefefe ffffff f2f1f6 f5f5f8 f5f5f8 d3d2d9 76757c 5f5d67 edebf2 e6e4ed',
            'blue' => 'f6f7fa f6f7fa fdfefe ffffff f0f2f6 f3f5f8 f3f5f8 d0d3d9 72767d 5a5f68 e9edf3 e2e6ed',
            'teal' => 'f5f8f8 f5f8f8 fdfefe ffffff eef3f3 f2f6f5 f2f6f5 cdd5d4 6e7877 556260 e6efed dee8e7',
            'green' => 'f5f8f6 f5f8f6 fdfefd ffffff eff3f0 f3f6f4 f3f6f4 cfd5d0 717873 58625b e8eeea e1e8e3',
            'amber' => 'f9f7f4 f9f7f4 fefefd ffffff f4f2ee f6f5f2 f6f5f2 d6d3cd 7a756e 645e55 f0ece6 eae5de',
            'orange' => 'faf7f5 faf7f5 fefdfd ffffff f6f1ef f7f4f3 f7f4f3 d8d2ce 7c7470 675d57 f2ebe7 ece4df',
            'rose' => 'faf6f7 faf6f7 fefdfe ffffff f6f0f1 f8f4f4 f8f4f4 d9d1d2 7d7374 685b5d f2eaeb ede3e4',
        ],
        'dark' => [
            'purple' => '0b0914 100e16 191621 211d2a 120f19 14111a 15121c 3b3447 84778f a296ae 2a2435 322a3f',
            'blue' => '080b10 0c1015 15181e 1c2025 0e1116 101318 111419 333842 777d88 959ca7 24282e 2b2f35',
            'teal' => '060d0c 091110 121a19 192120 0b1312 0d1514 0e1615 2e3b39 72817f 90a09d 212a28 28312f',
            'green' => '070c09 0b110d 141a16 1b211d 0d130f 0f1410 101612 313b34 758078 949f97 232925 2a302c',
            'amber' => '0e0a06 120f0a 1b1812 231f19 14100b 16120d 17130e 3d372e 837c71 a29b90 2b2721 322e28',
            'orange' => '0f0907 140e0b 1d1713 241e1b 15100c 17110e 19120f 403630 867a74 a69992 2d2622 342d29',
            'rose' => '0f090a 140d0e 1d1617 251d1e 160f10 181112 191213 413436 87797b a6979a 2d2526 342c2d',
        ],
    ];

    /** Columns: action, top, hover, active, on-action, light ink, dark ink, light border, dark border, dark progress. */
    private const ACCENTS = [
        'purple' => '7c3aed 8545e6 6d28d9 5b21b6 ffffff 6d28d9 c4a6ee 6d28d9 ad83e8 b18be7',
        'blue' => '2864c5 3570ce 2257ac 1c4791 ffffff 235bac a0c2f4 2257ac 80a8e5 8cb3ed',
        'teal' => '087c73 0a7e75 086b64 075b55 ffffff 086b64 8cd1c6 086b64 66b7aa 72c1b6',
        'green' => '267a48 2c7f4e 21693e 1c5935 ffffff 21693e a1d0ab 21693e 78ad87 85c495',
        'amber' => 'e4b34f e9bd64 dda53a d0992b 33250c 805515 e6c17c 956719 b88a35 dfb765',
        'orange' => 'f09a60 f3a674 e98c4b de7e3a 362014 964618 efb389 af5828 ce8b5a e5a471',
        'rose' => 'b83d5b bf4765 a33350 8d2b44 ffffff a33350 ecaabc a33350 d8869a dd91a6',
    ];

    /** Columns: light accent, light selected, dark accent, dark selected. */
    private const TINTS = [
        'purple' => 'f5f2ff eeeaff 241c38 2e214a', 'blue' => 'eff5fc e5eefb 1a2331 1e2b40',
        'teal' => 'eff6f5 e5f0ef 162624 18302d', 'green' => 'eff6f1 e6f0e8 19261d 1d2f23',
        'amber' => 'fbf7f0 f9f2e7 2d2618 3c301c', 'orange' => 'fcf4f0 fbeee7 302219 402a1d',
        'rose' => 'fcf2f4 fbeaed 301e21 3e2429',
    ];

    public static function mode(string $mode): string
    {
        if (! in_array($mode, ['light', 'dark'], true)) {
            throw new InvalidArgumentException('Theme mode must be light or dark.');
        }

        return $mode;
    }

    /** @return array<string, string> */
    public static function common(): array
    {
        return self::prefix(self::COMMON);
    }

    /** @return array<string, string> */
    public static function neutrals(string $preset, string $mode): array
    {
        return self::row('browser-chrome bg surface surface-raised surface-sunken footer-surface settings-surface border control-border control-border-hover surface-hover surface-active', self::NEUTRALS[self::mode($mode)][$preset]);
    }

    /** @return array<string, string> */
    public static function authored(string $preset, string $mode): array
    {
        $light = self::mode($mode) === 'light';
        [$solid, $top, $hover, $active, $ink, $lightInk, $darkInk, $lightBorder, $darkBorder, $progress] = array_map(static fn (string $v): string => '#'.$v, explode(' ', self::ACCENTS[$preset]));
        $tints = explode(' ', self::TINTS[$preset]);

        return [...self::neutrals($preset, $mode), ...self::prefix([
            'accent-text' => $light ? $lightInk : $darkInk,
            'accent-surface' => '#'.$tints[$light ? 0 : 2], 'selected-surface' => '#'.$tints[$light ? 1 : 3],
            'action' => $solid, 'action-hover' => $hover, 'action-active' => $active, 'on-action' => $ink,
            'action-bg' => $light ? $solid : "linear-gradient(180deg, $top, $solid)",
            'action-border' => $light ? $lightBorder : $darkBorder, 'choice-border' => $light ? $lightInk : $darkBorder,
            'progress-fill' => $light ? $lightInk : $progress,
        ])];
    }

    /** @return array<string, string> */
    public static function fixed(string $mode): array
    {
        $light = self::mode($mode) === 'light';
        $tokens = self::row('text text-muted text-subtle disabled-border disabled-surface disabled-text swatch-border', $light
            ? '262230 5e576a 62596d d8d2e0 ece8f1 777080 777080'
            : 'f4f1fa b5abbf ada3b7 443a50 2b2434 a89cb5 9c92ab');
        $status = $light
            ? '21693e edf7f0 34824e 805515 fbf4e5 986b26 b4233f fceef0 c34359 235bac edf3fc 4675b8'
            : 'a1d0ab 1d3025 79b688 e6c17c 352b1b c09d58 f0a7b9 3b242d ca7d91 a0c2f4 1e2c42 789ecd';
        $tokens += self::row('success success-surface success-border warning warning-surface warning-border danger danger-surface danger-border info info-surface info-border', $status);
        $tokens += self::row('editor-bg editor-gutter-bg editor-gutter-fg editor-gutter-active-bg editor-gutter-active-fg editor-border editor-active-line editor-selection editor-selection-inactive editor-matching-bracket editor-search-match editor-caret editor-bracket-border', $light
            ? 'fcfbfe f3f1f7 5c606d eee9f6 292532 d7d2e0 24203305 ece6f7 eeebf4 eee9f6 f7edd6 292532 783ab0'
            : '120f19 191621 ada6b8 30263f eceaf2 3b3447 ffffff05 332a46 282233 30263f 3a3021 eceaf2 cbacf5');
        $tokens += self::row('editor-fg editor-keyword editor-string editor-number editor-constant editor-function editor-property editor-type editor-tag editor-attribute editor-comment editor-punctuation editor-operator editor-meta editor-regexp editor-escape editor-link editor-heading editor-invalid', $light
            ? '292532 783ab0 176b45 925016 aa355a 245fa5 086976 744a97 a73558 8a4b17 5c606d 565c6a 565c6a 765a17 a03652 964712 245fa5 6e38b0 b4233f'
            : 'eceaf2 cbacf5 91ceac e7bc87 f3aec2 9abff5 89c8d3 d6b8f0 f0a7b9 e8c095 ada6b8 aba5b7 beb7cc dcc088 f3abc0 edc49a 9abff5 ceaeff f5a3b5');
        $tokens += self::prefix([
            'color-scheme' => $mode, 'switch-thumb-off' => '#ffffff',
            'action-shadow' => $light ? '0 1px 2px rgb(24 20 32 / 0.10)' : 'inset 0 1px 0 rgb(255 255 255 / 0.08), 0 2px 4px rgb(0 0 0 / 0.22)',
            'action-hover-shadow' => $light ? '0 2px 4px rgb(24 20 32 / 0.12)' : 'inset 0 1px 0 rgb(255 255 255 / 0.08), 0 3px 6px rgb(0 0 0 / 0.26)',
            'action-active-shadow' => $light ? 'inset 0 1px 2px rgb(24 20 32 / 0.12)' : 'inset 0 1px 2px rgb(0 0 0 / 0.20)',
            'shadow-panel' => $light ? '0 8px 24px rgb(24 20 32 / 0.06), 0 1px 3px rgb(24 20 32 / 0.04)' : '0 16px 40px rgb(0 0 0 / 0.16)',
            'shadow-popover' => $light ? '0 12px 28px rgb(24 20 32 / 0.10), 0 2px 6px rgb(24 20 32 / 0.06)' : '0 14px 32px rgb(0 0 0 / 0.36), 0 2px 6px rgb(0 0 0 / 0.16)',
            'dialog-scrim' => $light ? 'rgb(24 20 32 / 0.34)' : 'rgb(8 6 14 / 0.68)',
            'drawer-scrim' => $light ? 'rgb(24 20 32 / 0.24)' : 'rgb(8 6 14 / 0.52)',
            'progress-sheen' => $light ? 'transparent' : 'rgb(255 255 255 / 0.04)',
            'progress-sheen-peak' => $light ? 'transparent' : 'rgb(255 255 255 / 0.12)',
        ]);
        foreach (['soft' => ['0.06', '0.10'], 'medium' => ['0.10', '0.18'], 'strong' => ['0.14', '0.24'], 'heavy' => ['0.18', '0.42'], 'tooltip' => ['0.12', '0.24'], 'toast' => ['0.12', '0.25']] as $name => $alpha) {
            $tokens['--fb-shadow-'.$name] = 'rgb('.($light ? '24 20 32 / '.$alpha[0] : '0 0 0 / '.$alpha[1]).')';
        }
        foreach (['-soft' => '0.035', '' => '0.04', '-hover' => '0.06', '-action' => '0.08', '-dialog' => '0.025'] as $name => $alpha) {
            $tokens['--fb-shadow-highlight'.$name] = $light ? 'transparent' : "rgb(255 255 255 / $alpha)";
        }
        $channels = $light ? '36 32 51' : '255 255 255';
        foreach (['01' => '0.004', '02' => '0.008', '03' => '0.012', '04' => '0.016', '05' => '0.02', '06' => '0.024', '07' => '0.028', '09' => '0.032', '10' => '0.04', 'hover' => $light ? '0.03' : '0.04'] as $name => $alpha) {
            $tokens['--fb-wash-'.$name] = "rgb($channels / $alpha)";
        }
        foreach (['faint' => '0.05', 'soft' => '0.07', 'strong' => '0.1', 'cli' => '0.08'] as $name => $alpha) {
            $tokens['--fb-line-'.$name] = "rgb($channels / $alpha)";
        }

        return $tokens;
    }

    /** @return array<string, string> */
    public static function artwork(string $seed, string $endpoint, string $mode, bool $default): array
    {
        $light = self::mode($mode) === 'light';
        $tip = $light ? Color::mixOKLab($endpoint, '#000000', 0.90) : $endpoint;
        $bright = Color::mixOKLab($seed, '#ffffff', 0.82);
        $fold = Color::mixOKLab($endpoint, '#ffffff', $light ? 0.72 : 0.50);
        if ($default && ! $light) {
            $tip = '#c75bfa';
            $bright = '#a855f7';
            $fold = '#e7a1ff';
        }
        $start = Color::mixOKLab($seed, '#000000', 0.82);
        $first = Color::mixOKLab($start, $seed, 0.5);
        $second = Color::mixOKLab($seed, $tip, 0.5);

        return self::prefix([
            'brand' => $seed, 'brand-bright' => $bright, 'brand-tip' => $tip, 'brand-fold' => $fold,
            'brand-gradient' => "linear-gradient(135deg, $start 0%, $first 27.5%, $seed 55%, $second 77.5%, $tip 100%)",
        ]);
    }

    /** @return array<string, string> */
    public static function aliases(): array
    {
        $groups = [
            'text' => 'secondary-text rail-text settings-text settings-label tabs-text cli-command error-text report-heading report-label',
            'text-muted' => 'pill-text cli-icon cli-option cli-value cli-label error-muted report-description report-optional',
            'accent-text' => 'choice-text cli-accent share-mark-text error-signal os-accent focus',
            'border' => 'rail-border settings-border tabs-border file-mark-border note-border card-border share-border cli-border report-border error-badge-border password-menu-border row-hover-border',
            'control-border' => 'secondary-border dropzone-border dropzone-mark-border password-input-border inbox-empty-border inbox-button-border scrollbar switch-track-off',
            'control-border-hover' => 'secondary-hover-border dropzone-mark-hover-border password-hover-border inbox-button-hover-border cli-hover-border error-hover-border',
            'focus' => 'dropzone-hover-border password-open-border password-focus',
            'surface' => 'secondary-surface row-surface report-surface',
            'surface-hover' => 'secondary-hover-surface row-hover-surface password-hover-surface error-hover-surface',
            'surface-raised' => 'password-menu-surface error-surface', 'surface-sunken' => 'tabs-surface account-rail',
            'selected-surface' => 'choice-surface password-open-surface inbox-key-surface',
            'choice-border' => 'inbox-key-border', 'accent-surface' => 'choice-wash',
            'settings-surface' => 'note-tab-surface', 'danger-border' => 'alert-border',
            'danger-surface' => 'alert-surface', 'danger' => 'report-error', 'editor-selection' => 'selection',
            'action-hover' => 'action-hover-bg', 'action-active' => 'action-active-bg', 'on-action' => 'switch-thumb-checked',
        ];
        $aliases = [];
        foreach ($groups as $target => $names) {
            foreach (explode(' ', $names) as $name) {
                $aliases['--fb-'.$name] = '--fb-'.$target;
            }
        }

        return $aliases;
    }

    /** @return array<string, string> */
    public static function types(): array
    {
        $tokens = [...self::common(), ...self::fixed('dark'), ...self::authored('purple', 'dark'), ...ThemeEffects::tokens(self::SEEDS['purple'], 'dark', self::authored('purple', 'dark')), ...self::artwork(self::SEEDS['purple'], self::ENDPOINTS['purple'], 'dark', true), ...self::aliases(), '--fb-progress-track' => '', '--fb-focus-ring' => ''];
        $types = array_fill_keys(array_keys($tokens), 'color');
        foreach (array_keys(self::common()) as $name) {
            $types[$name] = 'non-color';
        }
        $types['--fb-color-scheme'] = 'non-color';
        foreach (explode(' ', 'action-bg action-hover-bg action-active-bg brand-gradient note-surface share-mark-surface') as $name) {
            $types['--fb-'.$name] = 'background';
        }
        foreach (explode(' ', 'ambient-glow dropzone-glow dropzone-mark dropzone-mark-hover') as $name) {
            $types['--fb-'.$name] = 'image';
        }
        foreach (explode(' ', 'shadow-panel shadow-popover action-shadow action-hover-shadow action-active-shadow focus-ring') as $name) {
            $types['--fb-'.$name] = 'shadow';
        }

        return $types;
    }

    /** @param array<string, string> $values
     * @return array<string, string>
     */
    private static function prefix(array $values): array
    {
        $result = [];
        foreach ($values as $name => $value) {
            $result['--fb-'.$name] = $value;
        }

        return $result;
    }

    /** @return array<string, string> */
    private static function row(string $names, string $colors): array
    {
        return self::prefix(array_combine(explode(' ', $names), array_map(static fn (string $color): string => '#'.$color, explode(' ', $colors))));
    }
}
