<?php

declare(strict_types=1);

namespace App\Services;

use Illuminate\Http\Client\ConnectionException;
use Illuminate\Http\Client\RequestException;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Http;
use UnexpectedValueException;

/**
 * @phpstan-type Installer array{os: string, architecture: string, format: string, name: string, url: string, size: int}
 * @phpstan-type Release array{version: string, notes_url: string, assets: list<Installer>}
 * @phpstan-type Result array{state: 'available'|'unavailable'|'error', release: Release|null}
 */
class DesktopReleases
{
    private const string Repository = 'Vented-Labs/filebeam';

    /** @return Result */
    public function latest(): array
    {
        $key = 'desktop-releases:v1';

        try {
            return Cache::remember($key, 300, fn (): array => $this->fetch());
        } catch (ConnectionException|RequestException|UnexpectedValueException) {
            $result = ['state' => 'error', 'release' => null];
            Cache::put($key, $result, 30);

            return $result;
        }
    }

    /** @return Result */
    private function fetch(): array
    {
        $releases = Http::acceptJson()
            ->withHeaders(['X-GitHub-Api-Version' => '2022-11-28'])
            ->connectTimeout(3)
            ->timeout(8)
            ->withoutRedirecting()
            ->get('https://api.github.com/repos/'.self::Repository.'/releases', ['per_page' => 100])
            ->throw()
            ->json();

        if (! is_array($releases) || ! array_is_list($releases)) {
            throw new UnexpectedValueException('Invalid desktop release response.');
        }

        $latest = null;
        foreach ($releases as $release) {
            if (! is_array($release)
                || ($release['draft'] ?? true) !== false
                || ($release['prerelease'] ?? true) !== false
                || ! is_string($tag = $release['tag_name'] ?? null)
                || preg_match('/^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/', $tag) !== 1
                || ! is_array($release['assets'] ?? null)) {
                continue;
            }

            $assets = $this->installers($tag, $release['assets']);
            if ($assets === [] || ($latest !== null && version_compare(substr($tag, 1), $latest['version'], '<='))) {
                continue;
            }

            $latest = [
                'version' => substr($tag, 1),
                'notes_url' => 'https://github.com/'.self::Repository.'/releases/tag/'.$tag,
                'assets' => $assets,
            ];
        }

        return ['state' => $latest === null ? 'unavailable' : 'available', 'release' => $latest];
    }

    /**
     * @param  array<mixed>  $assets
     * @return list<Installer>
     */
    private function installers(string $tag, array $assets): array
    {
        $installers = [];
        foreach ([
            ['linux', 'x86_64', 'AppImage'],
            ['linux', 'aarch64', 'AppImage'],
            ['macos', 'x86_64', 'dmg'],
            ['macos', 'aarch64', 'dmg'],
            ['windows', 'x86_64', 'exe'],
        ] as [$os, $architecture, $format]) {
            $name = "filebeam-desktop-$tag-$os-$architecture".($os === 'windows' ? '-setup' : '').'.'.$format;
            $url = 'https://github.com/'.self::Repository.'/releases/download/'.$tag.'/'.$name;
            foreach ($assets as $asset) {
                if (is_array($asset)
                    && ($asset['name'] ?? null) === $name
                    && ($asset['state'] ?? null) === 'uploaded'
                    && ($asset['browser_download_url'] ?? null) === $url
                    && is_int($size = $asset['size'] ?? null) && $size > 0) {
                    $installers[] = ['os' => $os, 'architecture' => $architecture, 'format' => $format, 'name' => $name, 'url' => $url, 'size' => $size];
                    break;
                }
            }
        }

        return $installers;
    }
}
