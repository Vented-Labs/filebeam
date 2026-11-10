<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Http\Requests\ChangelogIndexRequest;
use App\Models\ChangelogRelease;
use App\Models\ChangelogSyncState;
use App\Services\Changelog\ReleaseNodeTree;
use App\Services\Changelog\ReleaseSynchronizer;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Http\JsonResponse;
use Illuminate\Support\Str;
use Inertia\Inertia;
use Inertia\Response;

class ChangelogController extends Controller
{
    private const array Summary = ['id', 'upstream_id', 'tag', 'name', 'author', 'html_url', 'prerelease', 'published_at', 'renderer_version', 'document', 'body'];

    public function __construct(private readonly ReleaseSynchronizer $documents) {}

    public function index(ChangelogIndexRequest $request): Response
    {
        return Inertia::render('Changelog/Index', $this->listing($request));
    }

    /** The release list for native clients, with the same filters and paging as the page. */
    public function apiIndex(ChangelogIndexRequest $request): JsonResponse
    {
        $listing = $this->listing($request);

        return response()->json(['data' => [
            'releases' => $listing['releases'],
            'filters' => $listing['filters'],
            'versions' => $listing['versions'](),
            'has_prereleases' => $listing['has_prereleases'](),
            'archive' => $listing['archive'](),
            'source_url' => $listing['source_url'],
        ]], 200, ['Cache-Control' => 'public, max-age=300']);
    }

    /** One release with its neighbours, for native readers. */
    public function apiShow(string $tag): JsonResponse
    {
        $release = ChangelogRelease::query()->public()->where('tag', $tag)->select(self::Summary)->first();
        abort_if($release === null || $release->tag !== $tag, 404);
        $state = ChangelogSyncState::query()->find(ChangelogRelease::currentSource());

        return response()->json(['data' => [
            'release' => $this->release($release, $state),
            'newer' => $this->adjacent($release, 'newer'),
            'older' => $this->adjacent($release, 'older'),
            'archive' => $this->archive($state),
        ]], 200, ['Cache-Control' => 'public, max-age=300']);
    }

    /** @return array<string, mixed> */
    private function listing(ChangelogIndexRequest $request): array
    {
        $term = $request->term();
        $prereleases = $request->includesPrereleases();
        $state = ChangelogSyncState::query()->find(ChangelogRelease::currentSource());
        $perPage = (int) config('changelog.per_page');

        $page = ChangelogRelease::query()->public($prereleases)->matching($term)->newestFirst()
            ->select(self::Summary)
            ->paginate($perPage, page: $request->pageNumber())
            ->withQueryString();
        $featured = $term === '' && $page->currentPage() === 1;

        return [
            'releases' => [
                'data' => $page->getCollection()->values()->map(fn (ChangelogRelease $release, int $index): array => [
                    ...$this->release($release, $state),
                    'featured' => $featured && $index === 0,
                ])->all(),
                'current_page' => $page->currentPage(),
                'last_page' => $page->lastPage(),
                'total' => $page->total(),
                'prev_url' => $page->previousPageUrl(),
                'next_url' => $page->nextPageUrl(),
            ],
            'filters' => ['q' => $term, 'prereleases' => $prereleases],
            'versions' => fn (): array => $this->versions($prereleases, $perPage),
            'has_prereleases' => fn (): bool => ChangelogRelease::query()->public()->where('prerelease', true)->exists(),
            'archive' => fn (): array => $this->archive($state),
            'source_url' => 'https://github.com/'.config('changelog.repository').'/releases',
        ];
    }

    public function show(string $tag): Response
    {
        $release = ChangelogRelease::query()->public()->where('tag', $tag)->select(self::Summary)->first();
        abort_if($release === null || $release->tag !== $tag, 404);

        return $this->reader($release);
    }

    public function byId(int $githubReleaseId): Response
    {
        return $this->reader(ChangelogRelease::query()->public()->where('upstream_id', $githubReleaseId)->select(self::Summary)->firstOrFail());
    }

    private function reader(ChangelogRelease $release): Response
    {
        $state = ChangelogSyncState::query()->find(ChangelogRelease::currentSource());
        $adjacent = fn (string $direction): ?array => $this->adjacent($release, $direction);

        return Inertia::render('Changelog/Show', [
            'release' => $this->release($release, $state),
            'newer' => $adjacent('newer'),
            'older' => $adjacent('older'),
            'archive' => $this->archive($state),
        ]);
    }

    /** @return array<string, mixed> */
    private function release(ChangelogRelease $release, ?ChangelogSyncState $state): array
    {
        $release = $this->documents->prepare($release);
        $title = $release->title();

        return [
            'id' => $release->upstream_id,
            'tag' => $release->tag,
            'title' => $title,
            'titled' => $title !== $release->tag,
            'url' => $release->url(),
            'html_url' => $release->html_url,
            'author' => $release->author !== null && ! str_ends_with($release->author, '[bot]') ? $release->author : null,
            'prerelease' => $release->prerelease,
            'latest' => $state?->latest_upstream_id !== null && $state->latest_upstream_id === $release->upstream_id,
            'published_at' => $release->published_at?->toIso8601String(),
            'document' => $release->document,
            'description' => $this->description($release),
        ];
    }

    /** @return array{tag: string, url: string}|null */
    private function adjacent(ChangelogRelease $release, string $direction): ?array
    {
        $newer = $direction === 'newer';
        $neighbour = ChangelogRelease::query()->public($release->prerelease)
            ->where(fn (Builder $query) => $query
                ->where('published_at', $newer ? '>' : '<', $release->published_at)
                ->orWhere(fn (Builder $tie) => $tie->where('published_at', $release->published_at)->where('upstream_id', $newer ? '>' : '<', $release->upstream_id)))
            ->when($newer, fn (Builder $query) => $query->orderBy('published_at')->orderBy('upstream_id'), fn (Builder $query) => $query->newestFirst())
            ->select(['upstream_id', 'tag'])
            ->first();

        return $neighbour === null ? null : ['tag' => $neighbour->tag, 'url' => $neighbour->url()];
    }

    /** @return list<array{tag: string, url: string, page: int, prerelease: bool}> */
    private function versions(bool $prereleases, int $perPage): array
    {
        $versions = [];
        $releases = ChangelogRelease::query()->public($prereleases)->newestFirst()
            ->select(['upstream_id', 'tag', 'prerelease'])
            ->limit(500)->get();
        foreach ($releases as $index => $release) {
            $versions[] = [
                'tag' => $release->tag,
                'url' => $release->url(),
                'page' => intdiv($index, $perPage) + 1,
                'prerelease' => $release->prerelease,
            ];
        }

        return $versions;
    }

    /** @return array{state: 'ready'|'empty'|'unavailable', stale: bool, checked_at: string|null} */
    private function archive(?ChangelogSyncState $state): array
    {
        $checked = $state?->last_success_at;
        $hasReleases = ChangelogRelease::query()->public()->exists();
        $stale = $checked === null || $checked->lt(now()->subHours((int) config('changelog.stale_after_hours')));

        return [
            'state' => $hasReleases ? 'ready' : ($checked === null ? 'unavailable' : 'empty'),
            'stale' => $hasReleases && $stale && (bool) config('changelog.sync'),
            'checked_at' => $checked?->toIso8601String(),
        ];
    }

    private function description(ChangelogRelease $release): string
    {
        foreach ((array) ($release->document['blocks'] ?? []) as $block) {
            if (is_array($block) && ($block['type'] ?? null) === 'content') {
                $text = trim((string) preg_replace('/\s+/u', ' ', ReleaseNodeTree::text((array) ($block['nodes'] ?? []))));
                if ($text !== '') {
                    return Str::limit($text, 160);
                }
            }
        }

        return 'Release notes for '.$release->tag.'.';
    }
}
