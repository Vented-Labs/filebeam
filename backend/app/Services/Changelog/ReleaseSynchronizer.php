<?php

declare(strict_types=1);

namespace App\Services\Changelog;

use App\Models\ChangelogRelease;
use App\Models\ChangelogSyncState;
use Carbon\CarbonImmutable;
use Filebeam\Updater\ActivityLock;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Log;
use RuntimeException;

/**
 * Mirrors public GitHub releases into the local database. A recent pass only adds or updates
 * what it sees; a full pass may hide records, but only after two complete scans miss them.
 */
class ReleaseSynchronizer
{
    private const int MissingScansBeforeHiding = 2;

    public function __construct(
        private readonly GitHubReleaseClient $client,
        private readonly ReleaseDocumentRenderer $renderer,
    ) {}

    /** @return array{status: 'synced'|'skipped'|'locked'|'backoff'|'failed', reason?: string, imported?: int, updated?: int, hidden?: int, complete?: bool} */
    public function sync(bool $full = false, bool $force = false): array
    {
        if (! config('changelog.sync')) {
            return ['status' => 'skipped', 'reason' => 'disabled'];
        }

        $source = ChangelogRelease::currentSource();
        $lock = Cache::lock('changelog-sync:'.$source, 600);
        if (! $lock->get()) {
            return ['status' => 'locked'];
        }

        $activity = new ActivityLock(storage_path('app/update-activity.lock'));
        $handle = null;

        try {
            if (config('version.distribution') === 'package') {
                try {
                    $handle = $activity->acquireShared();
                } catch (RuntimeException) {
                    return ['status' => 'skipped', 'reason' => 'updating'];
                }
            }

            $state = ChangelogSyncState::for($source);
            if (! $force && $state->next_attempt_at?->isFuture()) {
                return ['status' => 'backoff', 'reason' => (string) $state->last_error];
            }

            return $this->run($state, $full);
        } finally {
            if (is_resource($handle)) {
                $activity->release($handle);
            }
            $lock->release();
        }
    }

    /** Re-renders a stored release when its document predates the current renderer. */
    public function prepare(ChangelogRelease $release): ChangelogRelease
    {
        if ($release->renderer_version === $this->renderer->version() && is_array($release->document)) {
            return $release;
        }

        $rendered = $this->renderer->render((string) $release->upstream_id, $release->tag, $release->name, $release->body, $release->html_url);
        $release->forceFill([
            'document' => $rendered['document'],
            'search_text' => $rendered['search_text'],
            'renderer_version' => $this->renderer->version(),
        ])->save();

        return $release;
    }

    /** @return array{status: 'synced'|'failed', reason?: string, imported?: int, updated?: int, hidden?: int, complete?: bool} */
    private function run(ChangelogSyncState $state, bool $full): array
    {
        $started = microtime(true);
        $deadline = $started + (int) config('changelog.budget.seconds');
        $candidate = $state->generation + 1;
        $counts = ['imported' => 0, 'updated' => 0, 'hidden' => 0];
        $state->forceFill(['last_attempt_at' => now('UTC')])->save();

        try {
            if (! $this->client->repositoryIsPublic()) {
                $counts['hidden'] = $this->hideAll($state->source);
                throw new ChangelogSyncException('repository_not_public');
            }

            $latest = $this->client->latestId();
            $validators = $state->pages ?? [];
            $seen = [];
            $complete = false;
            $budget = $full ? (int) config('changelog.budget.pages') : 1;

            for ($page = 1; $page <= $budget; $page++) {
                if (microtime(true) > $deadline) {
                    break;
                }

                $saved = $validators[(string) $page] ?? null;
                $result = $this->client->page($page, $saved['etag'] ?? null);

                if ($result['status'] === 304 && is_array($saved)) {
                    $ids = $saved['ids'];
                    $next = $saved['next'];
                } else {
                    if ($result['status'] === 304) {
                        // The saved page body was evicted; fetch it again unconditionally.
                        $result = $this->client->page($page, null);
                    }
                    $ids = [];
                    foreach ($result['releases'] as $release) {
                        $id = $this->store($state->source, $release, $full ? $candidate : null, $counts);
                        if ($id !== null) {
                            $ids[] = $id;
                        }
                    }
                    $next = $result['next'];
                    if ($result['etag'] !== null) {
                        $validators[(string) $page] = ['etag' => $result['etag'], 'ids' => $ids, 'next' => $next];
                    }
                }

                if ($full) {
                    if (array_intersect_key($seen, array_flip($ids)) !== []) {
                        // Releases shifted across a page boundary mid-scan; this pass is not a reconciliation.
                        $next = false;
                        $seen = [];
                        break;
                    }
                    $seen += array_flip($ids);
                    if ($result['status'] === 304) {
                        ChangelogRelease::query()->where('source', $state->source)->whereIn('upstream_id', $ids)
                            ->update(['seen_generation' => $candidate, 'missing_scans' => 0]);
                    }
                }

                if (! $next) {
                    $complete = $seen !== [] || $page === 1;
                    break;
                }
            }

            if ($full && $complete) {
                $counts['hidden'] += $this->reconcile($state->source, $candidate);
            }

            $this->rerenderStale($state->source);

            $state->forceFill([
                'generation' => $full && $complete ? $candidate : $state->generation,
                'last_success_at' => now('UTC'),
                'last_full_success_at' => $full && $complete ? now('UTC') : $state->last_full_success_at,
                'latest_upstream_id' => $latest,
                'next_attempt_at' => null,
                'failures' => 0,
                'last_error' => null,
                'pages' => array_slice($validators, 0, (int) config('changelog.budget.pages'), true),
            ])->save();

            Log::info('changelog.sync', [
                'source' => $state->source,
                'mode' => $full ? 'full' : 'recent',
                'complete' => $complete,
                'duration_ms' => (int) ((microtime(true) - $started) * 1000),
                ...$counts,
            ]);

            return ['status' => 'synced', 'complete' => $complete, ...$counts];
        } catch (ChangelogSyncException $exception) {
            $failures = $state->failures + 1;
            $backoff = min(2 ** min($failures, 10) * 60, 6 * 3600) + random_int(0, 30);
            $state->forceFill([
                'failures' => $failures,
                'last_error' => $exception->reason,
                'next_attempt_at' => CarbonImmutable::now('UTC')->addSeconds(max($backoff, $exception->retryAfter ?? 0)),
            ])->save();

            Log::warning('changelog.sync.failed', [
                'source' => $state->source,
                'mode' => $full ? 'full' : 'recent',
                'reason' => $exception->reason,
                'duration_ms' => (int) ((microtime(true) - $started) * 1000),
            ]);

            return ['status' => 'failed', 'reason' => $exception->reason, ...$counts];
        }
    }

    /**
     * @param  array<mixed>  $release
     * @param  array{imported: int, updated: int, hidden: int}  $counts
     */
    private function store(string $source, array $release, ?int $generation, array &$counts): ?int
    {
        $id = $release['id'] ?? null;
        $tag = $release['tag_name'] ?? null;
        if (! is_int($id) || $id <= 0 || ! is_string($tag) || $tag === '' || strlen($tag) > 255) {
            return null;
        }

        $existing = ChangelogRelease::query()->where('source', $source)->where('upstream_id', $id)->first();
        $published = $release['published_at'] ?? null;

        // Drafts and unpublished releases are never stored; an existing mirror entry is suppressed.
        if (($release['draft'] ?? true) !== false || ! is_string($published)) {
            if ($existing?->visible) {
                $this->hide($existing);
                $counts['hidden']++;
            }

            return $id;
        }

        $repository = strtolower((string) config('changelog.repository'));
        $url = $release['html_url'] ?? null;
        if (! is_string($url) || ! str_starts_with(strtolower($url), 'https://github.com/'.$repository.'/releases/')) {
            $url = 'https://github.com/'.config('changelog.repository').'/releases/tag/'.rawurlencode($tag);
        }

        $name = is_string($release['name'] ?? null) ? mb_substr($release['name'], 0, 255) : null;
        $body = is_string($release['body'] ?? null) ? $release['body'] : null;
        $author = $release['author']['login'] ?? null;
        $author = is_string($author) && preg_match('/^[A-Za-z0-9-]{1,39}(?:\[bot\])?$/', $author) === 1 ? mb_substr($author, 0, 39) : null;
        $prerelease = ($release['prerelease'] ?? false) === true;

        try {
            $publishedAt = CarbonImmutable::parse($published)->utc();
        } catch (\Throwable) {
            return null;
        }

        $hash = hash('sha256', json_encode([$tag, $name, $body, $author, $url, $prerelease, $publishedAt->toIso8601String()], JSON_THROW_ON_ERROR));
        $attributes = [
            'visible' => true,
            'missing_scans' => 0,
            ...($generation !== null ? ['seen_generation' => $generation] : []),
        ];

        if ($existing !== null && $existing->content_hash === $hash && $existing->renderer_version === $this->renderer->version()) {
            $existing->forceFill($attributes)->save();

            return $id;
        }

        $rendered = $this->renderer->render((string) $id, $tag, $name, $body, $url);
        $values = [
            ...$attributes,
            'source' => $source,
            'upstream_id' => $id,
            'tag' => $tag,
            'name' => $name,
            'body' => $body,
            'author' => $author,
            'html_url' => $url,
            'prerelease' => $prerelease,
            'published_at' => $publishedAt,
            'content_hash' => $hash,
            'document' => $rendered['document'],
            'search_text' => $rendered['search_text'],
            'renderer_version' => $this->renderer->version(),
        ];

        if ($existing === null) {
            ChangelogRelease::query()->create($values);
            $counts['imported']++;
        } else {
            $existing->forceFill($values)->save();
            $counts['updated']++;
        }

        return $id;
    }

    private function reconcile(string $source, int $generation): int
    {
        return DB::transaction(function () use ($source, $generation): int {
            ChangelogRelease::query()->where('source', $source)->where('visible', true)
                ->where('seen_generation', '<', $generation)
                ->increment('missing_scans');

            $missing = ChangelogRelease::query()->where('source', $source)->where('visible', true)
                ->where('missing_scans', '>=', self::MissingScansBeforeHiding)->get();
            foreach ($missing as $release) {
                $this->hide($release);
            }

            return $missing->count();
        });
    }

    private function hide(ChangelogRelease $release): void
    {
        $release->forceFill(['visible' => false, 'body' => null, 'document' => null, 'search_text' => null])->save();
    }

    private function hideAll(string $source): int
    {
        return ChangelogRelease::query()->where('source', $source)->where('visible', true)
            ->update(['visible' => false, 'body' => null, 'document' => null, 'search_text' => null]);
    }

    private function rerenderStale(string $source): void
    {
        ChangelogRelease::query()->where('source', $source)->where('visible', true)
            ->where(fn ($query) => $query->whereNull('renderer_version')->orWhere('renderer_version', '!=', $this->renderer->version()))
            ->limit(100)->get()
            ->each(fn (ChangelogRelease $release) => $this->prepare($release));
    }
}
