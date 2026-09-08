<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use App\Support\Installation\InstallationState;
use Closure;
use Illuminate\Cache\FileStore;
use Illuminate\Cache\RateLimiter;
use Illuminate\Cache\Repository;
use Illuminate\Filesystem\Filesystem;
use Illuminate\Http\Request;
use Illuminate\Validation\ValidationException;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpKernel\Exception\HttpExceptionInterface;
use Throwable;

readonly class InstallationAccess
{
    public function __construct(private InstallationState $state) {}

    /** @param Closure(Request): Response $next
     * @throws HttpExceptionInterface
     */
    public function handle(Request $request, Closure $next): Response
    {
        abort_unless($this->state->isPending() || $this->state->canBootstrap(), 404);
        $host = trim($request->getHost(), '[]');
        $local = $host === 'localhost' || $host === '::1' || (filter_var($host, FILTER_VALIDATE_IP, FILTER_FLAG_IPV4) !== false && str_starts_with($host, '127.'));
        abort_unless($request->isSecure() || $local, 403, 'Use HTTPS to configure Filebeam.');

        $filesystem = new Filesystem;
        $filesystem->ensureDirectoryExists($this->state->directory(), 0700);
        $limiter = new RateLimiter(new Repository(new FileStore($filesystem, $this->state->directory().'/rate-limits')));
        $key = hash('sha256', (string) $request->ip());
        abort_if($limiter->tooManyAttempts($key, 40), 429, 'Too many installation requests. Try again in a minute.');
        $limiter->hit($key, 60);

        if (! $request->isMethod('GET')) {
            abort_unless($request->header('Origin') === $request->getSchemeAndHttpHost(), 403, 'Installation requests must be same-origin.');
            abort_if($request->header('Sec-Fetch-Site') === 'cross-site', 403);
            if ($request->routeIs('install.bootstrap')) {
                abort_unless($this->state->validChallenge((string) $request->header('X-Installation-Challenge')), 419, 'Reload the installer to generate a new bootstrap challenge.');
            } else {
                $hash = $this->state->read()['token_hash'] ?? null;
                abort_unless(is_string($hash) && strlen($hash) === 64 && hash_equals($hash, hash('sha256', (string) $request->header('X-Installation-Token'))), 401, 'The installation token is invalid.');
            }
        }

        try {
            return $next($request);
        } catch (ValidationException $exception) {
            return response()->json(['message' => 'Please check the installation settings.', 'errors' => $exception->errors()], 422);
        } catch (HttpExceptionInterface $exception) {
            throw $exception;
        } catch (Throwable) {
            // Driver exceptions can contain connection passwords; never report or echo them.
            return response()->json(['message' => 'Installation could not continue. Check server permissions and connection settings, then retry.'], 500);
        }
    }
}
