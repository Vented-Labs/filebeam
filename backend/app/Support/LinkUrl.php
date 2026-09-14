<?php

declare(strict_types=1);

namespace App\Support;

class LinkUrl
{
    public const MAX_LENGTH = 2048;

    /**
     * Absolute http(s) URLs only, without credentials, whitespace, or control characters.
     */
    public static function isAcceptable(string $url): bool
    {
        return $url !== ''
            && strlen($url) <= self::MAX_LENGTH
            && filter_var($url, FILTER_VALIDATE_URL) !== false
            && in_array(parse_url($url, PHP_URL_SCHEME), ['http', 'https'], true)
            && parse_url($url, PHP_URL_USER) === null
            && parse_url($url, PHP_URL_PASS) === null
            && ! preg_match('/[\x00-\x20\x7f]/', $url);
    }
}
