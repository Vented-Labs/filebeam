<?php

declare(strict_types=1);

namespace App\Support;

use App\Enums\SocialPlatform;
use App\Enums\TransferDriver;
use InvalidArgumentException;

/**
 * Normalizes one instance setting by its declared type. Null, '' and [] mean "inherit the fallback".
 */
class InstanceSettingValue
{
    public const MAX_TEXT_LENGTH = 120;

    /** @throws InvalidArgumentException */
    public static function normalize(string $type, mixed $value): mixed
    {
        if ($value === null || $value === '' || $value === []) {
            return null;
        }

        return match ($type) {
            'boolean' => self::boolean($value),
            'drivers' => self::drivers($value),
            'driver' => self::driver($value),
            'text' => self::text($value),
            'year' => self::year($value),
            'url' => self::url($value),
            'community_links' => self::communityLinks($value),
            default => throw new InvalidArgumentException("Unknown instance setting type [{$type}]."),
        };
    }

    private static function boolean(mixed $value): bool
    {
        if (is_bool($value)) {
            return $value;
        }

        if (in_array($value, ['1', 1, 'true'], true)) {
            return true;
        }

        if (in_array($value, ['0', 0, 'false'], true)) {
            return false;
        }

        throw new InvalidArgumentException('Select Enabled, Disabled, or Inherit.');
    }

    /** @return list<string> */
    private static function drivers(mixed $value): array
    {
        $known = array_map(static fn (TransferDriver $driver): string => $driver->value, TransferDriver::cases());
        $drivers = is_array($value) ? array_values(array_unique($value)) : [];

        if ($drivers === [] || array_filter($drivers, 'is_string') !== $drivers || array_diff($drivers, $known) !== []) {
            throw new InvalidArgumentException('Enable at least one known transfer driver.');
        }

        /** @var list<string> $drivers */
        return $drivers;
    }

    private static function driver(mixed $value): string
    {
        $driver = is_string($value) ? TransferDriver::tryFrom($value) : null;

        if ($driver === null) {
            throw new InvalidArgumentException('The default transfer driver must be a known driver.');
        }

        return $driver->value;
    }

    private static function text(mixed $value): string
    {
        $text = is_string($value) ? trim($value) : '';

        if ($text === '' || mb_strlen($text) > self::MAX_TEXT_LENGTH || preg_match('/[\x00-\x1f\x7f]/', $text)) {
            throw new InvalidArgumentException('Enter a single line of at most '.self::MAX_TEXT_LENGTH.' characters.');
        }

        return $text;
    }

    private static function year(mixed $value): int
    {
        $year = is_int($value) ? $value : ((is_string($value) && ctype_digit($value)) ? (int) $value : null);

        if ($year === null || $year < 1970 || $year > 2100) {
            throw new InvalidArgumentException('Enter a four-digit year between 1970 and 2100.');
        }

        return $year;
    }

    private static function url(mixed $value): string
    {
        $url = is_string($value) ? trim($value) : '';

        if (! LinkUrl::isAcceptable($url)) {
            throw new InvalidArgumentException('Enter an absolute http or https URL without credentials.');
        }

        return $url;
    }

    /**
     * Rejects unknown platforms, repeated platforms, and anything that is not an absolute http(s) URL.
     *
     * @return list<array{platform: string, url: string}>
     */
    private static function communityLinks(mixed $links): array
    {
        if (! is_array($links) || ! array_is_list($links)) {
            throw new InvalidArgumentException('Community links must be a list of platform and URL pairs.');
        }

        $normalized = [];

        foreach ($links as $index => $link) {
            $ordinal = $index + 1;
            $platform = is_array($link) && is_string($link['platform'] ?? null) ? SocialPlatform::tryFrom($link['platform']) : null;

            if ($platform === null) {
                throw new InvalidArgumentException("Community link {$ordinal} must use a supported platform.");
            }

            if (isset($normalized[$platform->value])) {
                throw new InvalidArgumentException("{$platform->getLabel()} can only be linked once.");
            }

            $url = is_string($link['url'] ?? null) ? trim($link['url']) : '';

            if (! LinkUrl::isAcceptable($url)) {
                throw new InvalidArgumentException("The {$platform->getLabel()} link must be an absolute http or https URL without credentials.");
            }

            $normalized[$platform->value] = ['platform' => $platform->value, 'url' => $url];
        }

        return array_values($normalized);
    }
}
