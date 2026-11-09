<?php

declare(strict_types=1);

namespace App\Models;

use Carbon\CarbonImmutable;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\QueryException;
use Illuminate\Support\Facades\Cache;

/**
 * @property int $id
 * @property string $source
 * @property int $upstream_id
 * @property string $tag
 * @property string|null $name
 * @property string|null $body
 * @property string|null $author
 * @property string $html_url
 * @property bool $prerelease
 * @property bool $visible
 * @property CarbonImmutable|null $published_at
 * @property string $content_hash
 * @property string|null $renderer_version
 * @property array<string, mixed>|null $document
 * @property string|null $search_text
 * @property int $missing_scans
 * @property int $seen_generation
 */
#[Fillable(['source', 'upstream_id', 'tag', 'name', 'body', 'author', 'html_url', 'prerelease', 'visible', 'published_at', 'content_hash', 'renderer_version', 'document', 'search_text', 'missing_scans', 'seen_generation'])]
class ChangelogRelease extends Model
{
    /** Tags that can be used directly as a single URL segment. */
    private const string SafeTag = '/^[A-Za-z0-9][A-Za-z0-9._+-]{0,99}$/';

    public static function currentSource(): string
    {
        return 'github:'.strtolower((string) config('changelog.repository'));
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function public(Builder $query, bool $prereleases = true): void
    {
        $query->where('source', self::currentSource())
            ->where('visible', true)
            ->whereNotNull('published_at')
            ->when(! $prereleases, fn (Builder $releases) => $releases->where('prerelease', false));
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function newestFirst(Builder $query): void
    {
        $query->orderByDesc('published_at')->orderByDesc('upstream_id');
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function matching(Builder $query, string $term): void
    {
        $term = mb_strtolower(trim($term));
        if ($term === '') {
            return;
        }

        // Search input is literal: escape LIKE wildcards and bind the pattern.
        $pattern = '%'.str_replace(['!', '%', '_'], ['!!', '!%', '!_'], $term).'%';
        $query->whereRaw("search_text LIKE ? ESCAPE '!'", [$pattern]);
    }

    /** Release notes URL for an installed version when that release is mirrored locally. */
    public static function versionUrl(string $version): ?string
    {
        if (! config('changelog.enabled') || $version === '') {
            return null;
        }

        try {
            return Cache::remember('changelog:version-url:'.self::currentSource().':'.$version, 300, function () use ($version): string {
                $release = self::query()->public()->whereIn('tag', ['v'.$version, $version])->newestFirst()->first(['upstream_id', 'tag']);

                // Cache misses as an empty string so a missing release does not query on every request.
                return $release?->url() ?? '';
            }) ?: null;
        } catch (QueryException) {
            return null;
        }
    }

    public function url(): string
    {
        return preg_match(self::SafeTag, $this->tag) === 1
            ? route('changelog.show', ['tag' => $this->tag])
            : route('changelog.by-id', ['githubReleaseId' => $this->upstream_id]);
    }

    public function title(): string
    {
        $name = trim((string) $this->name);

        return $name === '' ? $this->tag : $name;
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'upstream_id' => 'integer',
            'prerelease' => 'boolean',
            'visible' => 'boolean',
            'published_at' => 'immutable_datetime',
            'document' => 'array',
            'missing_scans' => 'integer',
            'seen_generation' => 'integer',
        ];
    }
}
