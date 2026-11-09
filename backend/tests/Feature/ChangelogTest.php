<?php

declare(strict_types=1);

use App\Models\ChangelogRelease;
use App\Models\ChangelogSyncState;
use App\Services\Changelog\ReleaseSynchronizer;
use Carbon\CarbonImmutable;
use Illuminate\Support\Facades\Http;
use Inertia\Testing\AssertableInertia as Assert;

/** @return array<string, mixed> */
function githubRelease(int $id, string $tag, array $overrides = []): array
{
    return [
        'id' => $id,
        'tag_name' => $tag,
        'name' => $tag,
        'body' => "## What's Changed\n\n- Release $tag",
        'draft' => false,
        'prerelease' => false,
        'published_at' => CarbonImmutable::parse('2026-10-01')->addDays($id)->toIso8601String(),
        'html_url' => "https://github.com/Vented-Labs/filebeam/releases/tag/$tag",
        'author' => ['login' => 'macaws'],
        ...$overrides,
    ];
}

/**
 * One fake per test; scenarios change by updating this shared upstream state.
 *
 * @param  list<list<array<string, mixed>>>  $pages
 */
function fakeGithub(array $pages, ?int $latest = null, bool $public = true, ?int $status = null, bool $notModified = false): void
{
    $first = ! isset($GLOBALS['changelogUpstream']);
    $GLOBALS['changelogUpstream'] = compact('pages', 'latest', 'public', 'status', 'notModified');
    if (! $first) {
        return;
    }

    Http::preventStrayRequests();
    Http::fake(function ($request) {
        ['pages' => $pages, 'latest' => $latest, 'public' => $public, 'status' => $status, 'notModified' => $notModified] = $GLOBALS['changelogUpstream'];
        if ($status !== null) {
            return Http::response(['message' => 'unavailable'], $status, ['Retry-After' => '120']);
        }
        $url = $request->url();
        if (str_ends_with((string) parse_url($url, PHP_URL_PATH), '/releases/latest')) {
            return $latest === null ? Http::response(['message' => 'Not Found'], 404) : Http::response(['id' => $latest]);
        }
        if (str_contains($url, '/releases?')) {
            parse_str((string) parse_url($url, PHP_URL_QUERY), $query);
            $page = (int) ($query['page'] ?? 1);
            $etag = '"page-'.$page.'-'.md5((string) json_encode($pages[$page - 1] ?? [])).'"';
            if ($notModified) {
                expect($request->header('If-None-Match')[0] ?? null)->toBe($etag);

                return Http::response('', 304);
            }
            $headers = ['ETag' => $etag];
            if ($page < count($pages)) {
                $headers['Link'] = '<https://api.github.com/repositories/1/releases?per_page=100&page='.($page + 1).'>; rel="next"';
            }

            return Http::response($pages[$page - 1] ?? [], 200, $headers);
        }

        return Http::response(['private' => ! $public, 'visibility' => $public ? 'public' : 'private']);
    });
}

afterEach(function (): void {
    unset($GLOBALS['changelogUpstream']);
});

function synchronize(bool $full = true): array
{
    return app(ReleaseSynchronizer::class)->sync($full, force: true);
}

test('a full import mirrors every published page and ignores drafts', function (): void {
    fakeGithub([
        [githubRelease(3, 'v0.3.0'), githubRelease(4, 'v0.4.0', ['draft' => true, 'body' => 'secret draft'])],
        [githubRelease(2, 'v0.2.0'), githubRelease(1, 'v0.1.0', ['published_at' => null])],
    ], latest: 3);

    expect(synchronize())->toMatchArray(['status' => 'synced', 'complete' => true, 'imported' => 2]);
    expect(ChangelogRelease::query()->orderBy('upstream_id')->pluck('tag')->all())->toBe(['v0.2.0', 'v0.3.0'])
        ->and(ChangelogRelease::query()->where('body', 'like', '%secret%')->exists())->toBeFalse()
        ->and(ChangelogSyncState::query()->value('latest_upstream_id'))->toBe(3);

    $this->artisan('filebeam:sync-changelog --full --force')->assertSuccessful();
});

test('edited bodies are re-rendered and a private repository hides the feed', function (): void {
    fakeGithub([[githubRelease(3, 'v0.3.0')]]);
    synchronize();
    fakeGithub([[githubRelease(3, 'v0.3.0', ['body' => '## Fixed', 'name' => 'Better.'])]]);
    expect(synchronize(false))->toMatchArray(['status' => 'synced', 'updated' => 1]);
    expect(ChangelogRelease::query()->value('name'))->toBe('Better.');

    fakeGithub([[githubRelease(3, 'v0.3.0')]], public: false);
    expect(synchronize())->toMatchArray(['status' => 'failed', 'reason' => 'repository_not_public']);
    expect(ChangelogRelease::query()->where('visible', true)->count())->toBe(0);
});

test('releases missing from upstream are hidden only after two complete scans', function (): void {
    fakeGithub([[githubRelease(3, 'v0.3.0'), githubRelease(2, 'v0.2.0')]]);
    synchronize();

    fakeGithub([[githubRelease(3, 'v0.3.0')]]);
    synchronize(false);
    synchronize();
    expect(ChangelogRelease::query()->where('tag', 'v0.2.0')->value('visible'))->toBeTrue();

    synchronize();
    expect(ChangelogRelease::query()->where('tag', 'v0.2.0')->first())
        ->visible->toBeFalse()
        ->body->toBeNull();
});

test('failures preserve saved notes and back off', function (): void {
    fakeGithub([[githubRelease(3, 'v0.3.0')]]);
    synchronize();

    fakeGithub([[githubRelease(3, 'v0.3.0')]], status: 429);
    expect(synchronize())->toMatchArray(['status' => 'failed', 'reason' => 'rate_limited']);
    expect(ChangelogRelease::query()->where('visible', true)->count())->toBe(1);

    $state = ChangelogSyncState::query()->firstOrFail();
    expect($state->failures)->toBe(1)
        ->and($state->next_attempt_at?->greaterThan(now()->addSeconds(110)))->toBeTrue()
        ->and(app(ReleaseSynchronizer::class)->sync())->toMatchArray(['status' => 'backoff']);
    $this->artisan('filebeam:sync-changelog --force')->assertFailed();
});

test('a 304 page reuses saved validators without touching releases', function (): void {
    fakeGithub([[githubRelease(3, 'v0.3.0')]]);
    synchronize();

    fakeGithub([[githubRelease(3, 'v0.3.0')]], notModified: true);

    expect(synchronize())->toMatchArray(['status' => 'synced', 'complete' => true, 'imported' => 0, 'updated' => 0]);
    expect(ChangelogRelease::query()->where('visible', true)->count())->toBe(1);
});

test('the index lists the local mirror with search, pagination, and the upstream latest badge', function (): void {
    fakeGithub([array_map(fn (int $id): array => githubRelease($id, "v0.$id.0", $id === 2 ? ['body' => '100% literal_match'] : []), range(7, 1))], latest: 6);
    synchronize();

    $this->get('/changelog')->assertOk()->assertInertia(fn (Assert $page) => $page
        ->component('Changelog/Index')
        ->has('releases.data', 5)
        ->where('releases.data.0.tag', 'v0.7.0')
        ->where('releases.data.0.featured', true)
        ->where('releases.data.0.latest', false)
        ->where('releases.data.1.latest', true)
        ->where('releases.last_page', 2)
        ->where('versions.5.page', 2)
        ->where('archive.state', 'ready')
        ->missing('releases.data.0.body'));

    $this->get('/changelog?q=100%25')->assertInertia(fn (Assert $page) => $page->has('releases.data', 1)->where('releases.data.0.tag', 'v0.2.0'));
    $this->get('/changelog?q=literal_')->assertInertia(fn (Assert $page) => $page->has('releases.data', 1));
    $this->get('/changelog?q=l_teral')->assertInertia(fn (Assert $page) => $page->has('releases.data', 0));
    $this->get('/changelog?q='.str_repeat('a', 121))->assertSessionHasErrors('q');
});

test('release pages resolve real tags and return 404 for unknown or hidden releases', function (): void {
    fakeGithub([[githubRelease(3, 'v0.3.0'), githubRelease(2, 'v0.2.0'), githubRelease(1, 'cli/v1', ['name' => 'CLI'])]]);
    synchronize();

    $this->get('/changelog/v0.3.0')->assertOk()->assertInertia(fn (Assert $page) => $page
        ->component('Changelog/Show')
        ->where('release.tag', 'v0.3.0')
        ->where('newer', null)
        ->where('older.tag', 'v0.2.0'));
    $this->get('/changelog/id/1')->assertOk()->assertInertia(fn (Assert $page) => $page->where('release.tag', 'cli/v1')->where('release.url', route('changelog.by-id', ['githubReleaseId' => 1])));
    $this->get('/changelog/v9.9.9')->assertNotFound();

    ChangelogRelease::query()->where('tag', 'v0.2.0')->update(['visible' => false]);
    $this->get('/changelog/v0.2.0')->assertNotFound();
});

test('a cold mirror reports an unavailable archive instead of an empty changelog', function (): void {
    $this->get('/changelog')->assertOk()->assertInertia(fn (Assert $page) => $page->where('archive.state', 'unavailable')->has('releases.data', 0));
});

test('the shell links the installed version to its mirrored release notes', function (): void {
    config(['version.version' => '0.3.0']);
    fakeGithub([[githubRelease(3, 'v0.3.0')]]);
    synchronize();

    $this->get('/features')->assertOk()->assertInertia(fn (Assert $page) => $page
        ->component('Features')
        ->where('filebeam.changelog.enabled', true)
        ->where('filebeam.changelog.version_url', route('changelog.show', ['tag' => 'v0.3.0'])));
});

test('the first pass imports the whole archive and later recent passes read only the newest page', function (): void {
    fakeGithub([[githubRelease(5, 'v0.5.0')], [githubRelease(4, 'v0.4.0')], [githubRelease(3, 'v0.3.0')]]);

    expect(synchronize(false))->toMatchArray(['status' => 'synced', 'complete' => true, 'imported' => 3])
        ->and(ChangelogSyncState::query()->value('last_full_success_at'))->not->toBeNull();

    fakeGithub([[githubRelease(6, 'v0.6.0')], [githubRelease(5, 'v0.5.0')], [githubRelease(9, 'v0.0.9')]]);
    expect(synchronize(false))->toMatchArray(['status' => 'synced', 'complete' => false, 'imported' => 1]);
    expect(ChangelogRelease::query()->where('tag', 'v0.0.9')->exists())->toBeFalse();
});

test('synchronization is skipped while the changelog is disabled', function (): void {
    config(['changelog.enabled' => false]);
    fakeGithub([[githubRelease(3, 'v0.3.0')]]);

    expect(synchronize())->toBe(['status' => 'skipped', 'reason' => 'disabled'])
        ->and(ChangelogRelease::query()->exists())->toBeFalse();
    $this->artisan('filebeam:sync-changelog')->expectsOutputToContain('skipped: disabled')->assertSuccessful();
});
