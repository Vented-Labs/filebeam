<?php

declare(strict_types=1);

namespace App\Services\Changelog;

use GuzzleHttp\Exception\TransferException;
use Illuminate\Http\Client\ConnectionException;
use Illuminate\Http\Client\PendingRequest;
use Illuminate\Http\Client\Response;
use Illuminate\Support\Facades\Http;
use Psr\Http\Message\ResponseInterface;

/**
 * Read-only GitHub Releases client. The origin is fixed; only the configured owner/repository
 * pair varies, and pagination uses page numbers on that same path rather than server URLs.
 *
 * @phpstan-type Page array{status: 200|304, etag: string|null, releases: list<array<mixed>>, next: bool}
 */
class GitHubReleaseClient
{
    private const string Origin = 'https://api.github.com';

    public const int PerPage = 100;

    public function repositoryIsPublic(): bool
    {
        $response = $this->send('/repos/'.$this->repository());
        $data = $this->decode($response);

        return ($data['private'] ?? true) === false && ($data['visibility'] ?? 'public') === 'public';
    }

    public function latestId(): ?int
    {
        $response = $this->send('/repos/'.$this->repository().'/releases/latest', allowMissing: true);
        if ($response->status() === 404) {
            return null;
        }
        $id = $this->decode($response)['id'] ?? null;

        return is_int($id) && $id > 0 ? $id : null;
    }

    /** @return Page */
    public function page(int $page, ?string $etag): array
    {
        $response = $this->send(
            '/repos/'.$this->repository().'/releases',
            ['per_page' => self::PerPage, 'page' => $page],
            $etag,
        );
        $next = str_contains($response->header('Link'), 'rel="next"');

        if ($response->status() === 304) {
            return ['status' => 304, 'etag' => $etag, 'releases' => [], 'next' => $next];
        }

        $releases = $this->decode($response);
        if (! array_is_list($releases)) {
            throw new ChangelogSyncException('invalid_response');
        }

        return [
            'status' => 200,
            'etag' => $response->header('ETag') ?: null,
            'releases' => array_values(array_filter($releases, 'is_array')),
            'next' => $next,
        ];
    }

    private function repository(): string
    {
        return (string) config('changelog.repository');
    }

    /** @param array<string, int> $query */
    private function send(string $path, array $query = [], ?string $etag = null, bool $allowMissing = false): Response
    {
        $limit = (int) config('changelog.budget.page_bytes');

        try {
            $response = $this->request($etag)
                ->withOptions([
                    'on_headers' => function (ResponseInterface $response) use ($limit): void {
                        $length = $response->getHeaderLine('Content-Length');
                        if ($length !== '' && (int) $length > $limit) {
                            throw new ChangelogSyncException('too_large');
                        }
                    },
                ])
                ->get(self::Origin.$path, $query);
        } catch (ConnectionException) {
            throw new ChangelogSyncException('connection');
        } catch (TransferException $exception) {
            // Guzzle wraps exceptions thrown from on_headers.
            throw $exception->getPrevious() instanceof ChangelogSyncException ? $exception->getPrevious() : new ChangelogSyncException('connection');
        }

        $status = $response->status();
        if ($status === 304 || ($allowMissing && $status === 404) || $response->successful()) {
            if (strlen($response->body()) > $limit) {
                throw new ChangelogSyncException('too_large');
            }

            return $response;
        }

        $retryAfter = $this->retryAfter($response);
        throw new ChangelogSyncException(match (true) {
            $status === 429, $status === 403 && ($response->header('X-RateLimit-Remaining') === '0' || $retryAfter !== null) => 'rate_limited',
            $status === 401 => 'unauthorized',
            $status === 403 => 'forbidden',
            $status === 404 => 'not_found',
            $status >= 500 => 'upstream_unavailable',
            default => 'http_'.$status,
        }, $retryAfter);
    }

    private function request(?string $etag): PendingRequest
    {
        $headers = [
            'Accept' => 'application/vnd.github+json',
            'User-Agent' => 'Filebeam-Changelog/'.config('version.version'),
            'X-GitHub-Api-Version' => (string) config('changelog.api_version'),
        ];
        if ($etag !== null) {
            $headers['If-None-Match'] = $etag;
        }

        $request = Http::withHeaders($headers)->connectTimeout(5)->timeout(20)->withoutRedirecting();
        $token = config('changelog.token');

        return is_string($token) && $token !== '' ? $request->withToken($token) : $request;
    }

    /** @return array<mixed> */
    private function decode(Response $response): array
    {
        try {
            $data = json_decode($response->body(), true, 64, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            throw new ChangelogSyncException('invalid_response');
        }

        if (! is_array($data)) {
            throw new ChangelogSyncException('invalid_response');
        }

        return $data;
    }

    private function retryAfter(Response $response): ?int
    {
        $after = $response->header('Retry-After');
        if (ctype_digit($after)) {
            return min((int) $after, 86_400);
        }

        $reset = $response->header('X-RateLimit-Reset');
        if ($response->header('X-RateLimit-Remaining') === '0' && ctype_digit($reset)) {
            return max(60, min((int) $reset - time(), 86_400));
        }

        return null;
    }
}
