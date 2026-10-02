<?php

declare(strict_types=1);

namespace App\Support\Theming;

use RuntimeException;

final class MailTheme
{
    /** @return array<string, string> */
    public static function slots(Palette $instance): array
    {
        $l = $instance->tokens('light');
        $d = $instance->tokens('dark');

        return [
            'SHELL_BG' => $d['--fb-bg'], 'SHELL_TEXT' => $d['--fb-text'], 'CARD_BG' => '#ffffff',
            'BODY_TEXT' => $l['--fb-text'], 'MUTED_TEXT' => $l['--fb-text-muted'], 'BODY_LINK' => $l['--fb-accent-text'],
            'BORDER' => $l['--fb-border'], 'FOOTER_TEXT' => $d['--fb-text-muted'], 'FOOTER_LINK' => $d['--fb-accent-text'],
            'ACTION_BG' => $l['--fb-action'], 'ACTION_INK' => $l['--fb-on-action'],
            'PANEL_BG' => $l['--fb-accent-surface'], 'PANEL_TEXT' => $l['--fb-text'],
            'SUCCESS_BG' => '#21693e', 'DANGER_BG' => '#b4233f', 'SEMANTIC_INK' => '#ffffff',
        ];
    }

    public static function css(Palette $instance): string
    {
        $slots = [];
        foreach (self::slots($instance) as $name => $color) {
            if (! preg_match('/^#[0-9a-f]{6}$/D', $color)) {
                throw new RuntimeException('Invalid email color slot: '.$name);
            }
            $slots['{{FB_MAIL_'.$name.'}}'] = $color;
        }
        $css = strtr((string) file_get_contents(__DIR__.'/../../../resources/themes/mail.css'), $slots);
        if (str_contains($css, '{{') || preg_match('/(?:var|color-mix|oklch)\(/i', $css)) {
            throw new RuntimeException('Unresolved or unsupported email color.');
        }

        return $css;
    }
}
