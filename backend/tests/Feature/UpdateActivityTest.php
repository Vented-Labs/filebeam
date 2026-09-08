<?php

declare(strict_types=1);

use App\Http\Middleware\TrackUpdateActivity;
use App\Providers\AppServiceProvider;
use Filebeam\Updater\ActivityLock;
use Illuminate\Contracts\Http\Kernel;
use Illuminate\Contracts\Queue\Job;
use Illuminate\Http\Request;
use Illuminate\Queue\Events\JobAttempted;
use Illuminate\Queue\Events\JobExceptionOccurred;
use Illuminate\Queue\Events\JobProcessing;
use Illuminate\Queue\Events\Looping;
use Illuminate\Queue\Events\WorkerIdle;
use Illuminate\Queue\Events\WorkerStopping;
use Illuminate\Queue\WorkerOptions;
use Illuminate\Support\Facades\Event;
use Illuminate\Support\Facades\File;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpFoundation\StreamedResponse;

beforeEach(function (): void {
    $this->originalStoragePath = storage_path();
    $this->activityStoragePath = storage_path('framework/testing/update-activity-'.bin2hex(random_bytes(4)));
    File::ensureDirectoryExists($this->activityStoragePath.'/app', 0700);
    app()->useStoragePath($this->activityStoragePath);
    config()->set('version.distribution', 'package');
});

afterEach(function (): void {
    try {
        File::deleteDirectory($this->activityStoragePath);
    } finally {
        app()->useStoragePath($this->originalStoragePath);
        config()->set('version.distribution', 'source');
    }
});

test('allows concurrent activity readers and excludes an updater', function (): void {
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));
    $first = $lock->acquireShared();
    $second = $lock->acquireShared();

    try {
        expect(fn () => $lock->acquireExclusive(0))->toThrow(RuntimeException::class);
    } finally {
        $lock->release($second);
        $lock->release($first);
    }

    $updater = $lock->acquireExclusive(0);

    try {
        expect(fn () => $lock->acquireShared())->toThrow(RuntimeException::class);
    } finally {
        $lock->release($updater);
    }
});

test('releases a child process shared lock when the child exits', function (): void {
    $path = storage_path('app/update-activity.lock');
    $script = 'require_once '.var_export(base_path('../updater/ActivityLock.php'), true).'; '
        .'$handle = (new \\Filebeam\\Updater\\ActivityLock('.var_export($path, true).'))->acquireShared(); '
        .'fwrite(STDOUT, "locked\\n"); fgets(STDIN);';
    $process = proc_open([PHP_BINARY, '-r', $script], [
        0 => ['pipe', 'r'],
        1 => ['pipe', 'w'],
    ], $pipes);

    expect(is_resource($process))->toBeTrue();
    expect(fgets($pipes[1]))->toBe("locked\n");

    $lock = new ActivityLock($path);
    expect(fn () => $lock->acquireExclusive(0))->toThrow(RuntimeException::class);

    fclose($pipes[0]);
    fclose($pipes[1]);
    expect(proc_close($process))->toBe(0);

    $updater = $lock->acquireExclusive(0);
    $lock->release($updater);
});

test('waits for concurrent activity before allowing an updater', function (): void {
    $path = storage_path('app/update-activity.lock');
    $lock = new ActivityLock($path);
    $reader = $lock->acquireShared();
    $script = 'require_once '.var_export(base_path('../updater/ActivityLock.php'), true).'; '
        .'fwrite(STDOUT, "waiting\\n"); '
        .'$handle = (new \\Filebeam\\Updater\\ActivityLock('.var_export($path, true).'))->acquireExclusive(2); '
        .'fwrite(STDOUT, "acquired\\n");';
    $process = proc_open([PHP_BINARY, '-r', $script], [1 => ['pipe', 'w']], $pipes);

    expect(is_resource($process))->toBeTrue();
    expect(fgets($pipes[1]))->toBe("waiting\n");
    $lock->release($reader);
    expect(fgets($pipes[1]))->toBe("acquired\n");

    fclose($pipes[1]);
    expect(proc_close($process))->toBe(0);
});

test('rejects lock paths containing symlinks', function (): void {
    $target = storage_path('app/target.lock');
    $link = storage_path('app/update-activity.lock');
    File::put($target, 'target');
    symlink($target, $link);

    expect(fn () => (new ActivityLock($link))->acquireShared())->toThrow(RuntimeException::class);
    expect(File::get($target))->toBe('target');
});

test('releases a request lock after a streamed response terminates', function (): void {
    $middleware = new TrackUpdateActivity;
    $request = Request::create('/downloads/example');
    $response = $middleware->handle($request, fn (): StreamedResponse => new StreamedResponse(static function (): void {}));
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));

    expect(fn () => $lock->acquireExclusive(0))->toThrow(RuntimeException::class);

    $middleware->terminate($request, $response);
    $updater = $lock->acquireExclusive(0);
    $lock->release($updater);
});

test('releases the global middleware lock after kernel termination', function (): void {
    $request = Request::create('/up');
    $kernel = app(Kernel::class);
    $response = $kernel->handle($request);
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));

    expect(fn () => $lock->acquireExclusive(0))->toThrow(RuntimeException::class);

    $kernel->terminate($request, $response);
    $updater = $lock->acquireExclusive(0);
    $lock->release($updater);
});

test('releases a request lock when an exception response terminates', function (): void {
    $middleware = new TrackUpdateActivity;
    $request = Request::create('/downloads/example');

    expect(fn () => $middleware->handle($request, static function (): Response {
        throw new RuntimeException('Request failed.');
    }))->toThrow(RuntimeException::class);

    $lock = new ActivityLock(storage_path('app/update-activity.lock'));
    expect(fn () => $lock->acquireExclusive(0))->toThrow(RuntimeException::class);

    $middleware->terminate($request, new Response);
    $updater = $lock->acquireExclusive(0);
    $lock->release($updater);
});

test('allows the updater probe without consuming activity capacity', function (): void {
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));
    $updater = $lock->acquireExclusive(0);

    try {
        $response = (new TrackUpdateActivity)->handle(Request::create('/updater/probe'), fn (): Response => new Response('ok'));

        expect($response->getStatusCode())->toBe(200);
    } finally {
        $lock->release($updater);
    }
});

test('returns 503 for ordinary requests while an updater holds the lock', function (): void {
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));
    $updater = $lock->acquireExclusive(0);

    try {
        $response = (new TrackUpdateActivity)->handle(Request::create('/'), fn (): Response => new Response('ok'));

        expect($response->getStatusCode())->toBe(503);
    } finally {
        $lock->release($updater);
    }
});

test('releases an async job lock after its failed attempt is handled', function (): void {
    (new AppServiceProvider(app()))->boot();
    $job = Mockery::mock(Job::class);
    $job->shouldReceive('payload')->andReturn([]);

    Event::dispatch(new JobProcessing('database', $job));
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));
    expect(fn () => $lock->acquireExclusive(0))->toThrow(RuntimeException::class);

    Event::dispatch(new JobExceptionOccurred('database', $job, new RuntimeException('Job failed.')));
    expect(fn () => $lock->acquireExclusive(0))->toThrow(RuntimeException::class);

    Event::dispatch(new JobAttempted('database', $job, new RuntimeException('Job failed.')));
    $updater = $lock->acquireExclusive(0);
    $lock->release($updater);
});

test('does not reserve a queued job while an updater holds the activity lock', function (): void {
    (new AppServiceProvider(app()))->boot();
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));
    $updater = $lock->acquireExclusive(0);

    try {
        expect(Event::until(new Looping('database', 'default')))->toBeFalse();
    } finally {
        $lock->release($updater);
    }
});

test('transfers a loop lock to the reserved job and releases it after the attempt', function (): void {
    (new AppServiceProvider(app()))->boot();
    $job = Mockery::mock(Job::class);
    $job->shouldReceive('payload')->andReturn([]);
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));

    expect(Event::until(new Looping('database', 'default')))->toBeNull();
    expect(fn () => $lock->acquireExclusive(0))->toThrow(RuntimeException::class);

    Event::dispatch(new JobProcessing('database', $job));
    Event::dispatch(new JobAttempted('database', $job));

    $updater = $lock->acquireExclusive(0);
    $lock->release($updater);
});

test('releases an unreserved loop lock when a worker is idle or stopping', function (): void {
    (new AppServiceProvider(app()))->boot();
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));

    Event::until(new Looping('database', 'default'));
    expect(fn () => $lock->acquireExclusive(0))->toThrow(RuntimeException::class);

    Event::dispatch(new WorkerIdle('database', 'default', new WorkerOptions));
    $updater = $lock->acquireExclusive(0);
    $lock->release($updater);

    Event::until(new Looping('database', 'default'));
    Event::dispatch(new WorkerStopping);
    $updater = $lock->acquireExclusive(0);
    $lock->release($updater);
});

test('rejects transfer pruning while an updater holds the lock', function (): void {
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));
    $updater = $lock->acquireExclusive(0);

    try {
        $this->artisan('filebeam:prune-transfers')
            ->expectsOutput('Transfer pruning is unavailable while an update is in progress.')
            ->assertFailed();
    } finally {
        $lock->release($updater);
    }
});
