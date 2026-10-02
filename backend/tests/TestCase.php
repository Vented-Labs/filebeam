<?php

declare(strict_types=1);

namespace Tests;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Filesystem\Filesystem;
use Illuminate\Foundation\Application;
use Illuminate\Foundation\Testing\TestCase as BaseTestCase;
use LogicException;
use Symfony\Component\HttpFoundation\Request;

abstract class TestCase extends BaseTestCase
{
    /** @var list<string> */
    private array $themeDirectories = [];

    protected function setUp(): void
    {
        $this->resetTrustedRequestState();

        parent::setUp();
    }

    protected function tearDown(): void
    {
        try {
            parent::tearDown();
        } finally {
            foreach ($this->themeDirectories as $directory) {
                (new Filesystem)->deleteDirectory($directory);
            }
            $this->themeDirectories = [];
            $this->resetTrustedRequestState();
        }
    }

    public function createApplication(): Application
    {
        $app = parent::createApplication();
        // PHP tests must not depend on a developer's running SSR process.
        $app['config']->set('inertia.ssr.enabled', false);
        $themeDirectory = $app->storagePath('framework/testing/themes-'.bin2hex(random_bytes(8)));
        $this->themeDirectories[] = $themeDirectory;
        $app['config']->set('theme.storage_directory', $themeDirectory);
        $connection = $app['config']->get('database.default');
        $database = $app['config']->get("database.connections.{$connection}.database");

        if (! $app->environment('testing') || ! (
            ($connection === 'sqlite' && $database === ':memory:')
            || ($connection === 'mysql' && $database === 'filebeam_testing')
            || ($connection === 'pgsql' && $database === 'filebeam_testing')
        )) {
            throw new LogicException('Refusing to run database tests outside :memory: or filebeam_testing. Clear cached config and use the supplied PHPUnit configuration.');
        }

        Model::preventLazyLoading();

        return $app;
    }

    private function resetTrustedRequestState(): void
    {
        // Symfony keeps host and proxy trust in static state beyond Laravel's middleware flush.
        Request::setTrustedHosts([]);
        Request::setTrustedProxies([], Request::HEADER_X_FORWARDED_FOR | Request::HEADER_X_FORWARDED_HOST | Request::HEADER_X_FORWARDED_PORT | Request::HEADER_X_FORWARDED_PROTO | Request::HEADER_X_FORWARDED_PREFIX);
    }
}
