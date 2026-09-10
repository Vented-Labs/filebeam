<?php

declare(strict_types=1);

use App\Models\Plan;
use Database\Seeders\FilestoreSeeder;
use Inertia\Testing\AssertableInertia as Assert;

test('the homepage exposes the default plan and private response headers', function () {
    Plan::factory()->create(['slug' => 'default']);
    $this->get('/')
        ->assertOk()
        ->assertHeader('Referrer-Policy', 'no-referrer')
        ->assertHeader('X-Content-Type-Options', 'nosniff')
        ->assertInertia(fn (Assert $page) => $page->component('Welcome')
            ->where('filebeam.maximum_file_count', 20)
            ->where('filebeam.note_retention_hours', 720));
});

test('browser share URLs stay relative to the uploading instance', function () {
    Plan::factory()->create(['slug' => 'default']);
    $this->seed(FilestoreSeeder::class);
    config()->set('app.url', 'https://another-installation.example');
    $response = $this->postJson('/api/v1/transfers', [
        'kind' => 'files', 'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();
    expect($response->json('data.share_url'))->toBe('/'.$response->json('data.id'));
});

test('CLI installer instructions use public configuration without fetching artifacts', function () {
    Plan::factory()->create(['slug' => 'default']);
    config()->set('filebeam.cli.installer_url', null);
    $this->get('/')->assertInertia(fn (Assert $page) => $page
        ->where('filebeam.cli.installer_url', null)
        ->where('filebeam.cli.windows_installer_url', 'https://releases.filebeam.io/cli/install.ps1')
        ->where('filebeam.cli.installer_interpreter', 'sh')
        ->where('filebeam.cli.executable', 'beam'));

    config()->set('filebeam.cli.installer_url', 'https://releases.filebeam.test/cli/install.sh');
    $this->get('/')->assertInertia(fn (Assert $page) => $page
        ->where('filebeam.cli.installer_url', 'https://releases.filebeam.test/cli/install.sh'));
});
