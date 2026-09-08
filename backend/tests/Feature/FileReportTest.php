<?php

declare(strict_types=1);

use App\Enums\TransferStatus;
use App\Models\FileReport;
use App\Models\Transfer;
use App\Models\User;
use Illuminate\Support\Facades\RateLimiter;
use Illuminate\Support\Facades\Route;
use Inertia\Testing\AssertableInertia as Assert;

function reportPayload(Transfer $transfer, array $overrides = []): array
{
    return [
        'transfer_id' => $transfer->id,
        'category' => 'malware',
        'description' => 'This transfer appears to contain malicious software.',
        ...$overrides,
    ];
}

test('the public report form accepts only a safe transfer identifier query', function () {
    $transfer = Transfer::factory()->create();

    $this->get(route('reports.create', ['transfer_id' => $transfer->id, 'created' => 1]))
        ->assertOk()
        ->assertInertia(fn (Assert $page) => $page->component('Reports/Create')
            ->where('created', true)
            ->where('transferId', $transfer->id));

    $this->get('/reports/create?transfer_id=https%3A%2F%2Fexample.test%2F'.$transfer->id.'%23secret')
        ->assertOk()
        ->assertInertia(fn (Assert $page) => $page->where('transferId', null));
});

test('an anonymous visitor can report an available transfer', function () {
    $transfer = Transfer::factory()->create([
        'status' => TransferStatus::Available,
        'expires_at' => now()->addDay(),
    ]);

    $this->post(route('reports.store'), reportPayload($transfer, ['reporter_email' => 'reporter@example.test']))
        ->assertRedirect(route('reports.create', ['created' => 1, 'transfer_id' => $transfer->id]));

    $this->assertDatabaseHas('file_reports', [
        'transfer_id' => $transfer->id,
        'transfer_identifier' => $transfer->id,
        'reporter_id' => null,
        'reporter_email' => 'reporter@example.test',
        'category' => 'malware',
    ]);
});

test('spam submissions receive the same confirmation without creating a report', function () {
    $transfer = Transfer::factory()->create();

    $this->post(route('reports.store'), reportPayload($transfer, ['website' => 'https://spam.example']))
        ->assertRedirect(route('reports.create', ['created' => 1, 'transfer_id' => $transfer->id]));

    expect(FileReport::query()->count())->toBe(0);
});

test('missing, expired, and deleting transfers receive the same confirmation', function () {
    $missingTransfer = Transfer::factory()->make(['id' => '01K4Y8N9P0Q1R2S3T4V5W6X7Y8']);
    $expiredTransfer = Transfer::factory()->create(['status' => TransferStatus::Available, 'expires_at' => now()->subSecond()]);
    $deletingTransfer = Transfer::factory()->create(['status' => TransferStatus::Deleting, 'expires_at' => now()->addDay()]);

    foreach ([$missingTransfer, $expiredTransfer, $deletingTransfer] as $transfer) {
        $this->post(route('reports.store'), reportPayload($transfer))
            ->assertRedirect(route('reports.create', ['created' => 1, 'transfer_id' => $transfer->id]));
    }

    expect(FileReport::query()->count())->toBe(0);
});

test('report input is validated', function () {
    $this->from(route('reports.create'))->post(route('reports.store'), [
        'transfer_id' => 'not-an-ulid',
        'category' => 'invalid',
        'description' => 'short',
        'reporter_email' => str_repeat('a', 250).'@example.test',
        'website' => str_repeat('a', 201),
        'share_url' => 'https://filebeam.test/transfer#decryption-key',
    ])->assertRedirect(route('reports.create'))
        ->assertSessionHasErrors(['transfer_id', 'category', 'description', 'reporter_email', 'website'])
        ->assertSessionMissing('_old_input.share_url');
});

test('the authenticated web user is set by the server and unrecognized URL fields are not stored', function () {
    $user = User::factory()->create();
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Available, 'expires_at' => now()->addDay()]);
    $shareUrl = 'https://filebeam.test/'.$transfer->id.'#decryption-key';

    $this->actingAs($user, 'web')->post(route('reports.store'), reportPayload($transfer, [
        'reporter_id' => 999999,
        'share_url' => $shareUrl,
        'url' => $shareUrl,
    ]))->assertRedirect();

    $report = FileReport::query()->sole();
    expect($report->reporter_id)->toBe($user->id)
        ->and($report->getAttributes())->not->toHaveKey('share_url')
        ->and($report->description)->not->toContain('#decryption-key');
});

test('the report route uses the named file report throttle', function () {
    expect(Route::getRoutes()->getByName('reports.store')->gatherMiddleware())->toContain('throttle:file-reports');
});

test('a JSON report is accepted without an Inertia redirect', function () {
    $transfer = Transfer::factory()->create([
        'status' => TransferStatus::Available,
        'expires_at' => now()->addDay(),
    ]);

    $this->postJson(route('reports.store'), reportPayload($transfer, [
        'share_url' => 'https://filebeam.test/'.$transfer->id.'#decryption-key',
    ]))->assertAccepted()->assertExactJson(['status' => 'received']);

    $this->assertDatabaseHas('file_reports', [
        'transfer_id' => $transfer->id,
        'category' => 'malware',
    ]);
    expect(FileReport::query()->sole()->getAttributes())->not->toHaveKey('share_url');
});

test('expired and honeypot JSON reports receive the same confirmation', function () {
    $expiredTransfer = Transfer::factory()->create([
        'status' => TransferStatus::Available,
        'expires_at' => now()->subSecond(),
    ]);
    $availableTransfer = Transfer::factory()->create();

    $this->postJson(route('reports.store'), reportPayload($expiredTransfer))
        ->assertAccepted()
        ->assertExactJson(['status' => 'received']);
    $this->postJson(route('reports.store'), reportPayload($availableTransfer, [
        'website' => 'https://spam.example',
    ]))->assertAccepted()->assertExactJson(['status' => 'received']);

    expect(FileReport::query()->count())->toBe(0);
});

test('invalid JSON reports return inline field errors', function () {
    $this->postJson(route('reports.store'), [
        'transfer_id' => 'not-an-ulid',
        'category' => 'invalid',
        'description' => 'short',
    ])->assertUnprocessable()->assertJsonValidationErrors([
        'transfer_id',
        'category',
        'description',
    ]);
});

test('the report route rejects a sixth request in a minute', function () {
    $ip = '198.51.100.42';
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Available, 'expires_at' => now()->addDay()]);
    RateLimiter::clear('minute:'.$ip);
    RateLimiter::clear('hour:'.$ip);

    foreach (range(1, 5) as $request) {
        $this->withServerVariables(['REMOTE_ADDR' => $ip])
            ->post(route('reports.store'), reportPayload($transfer))
            ->assertRedirect();
    }

    $this->withServerVariables(['REMOTE_ADDR' => $ip])
        ->post(route('reports.store'), reportPayload($transfer))
        ->assertTooManyRequests();
});
