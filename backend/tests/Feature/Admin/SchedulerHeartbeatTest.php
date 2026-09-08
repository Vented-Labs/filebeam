<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Livewire\SchedulerHeartbeatAlert;
use App\Models\User;
use Filament\Facades\Filament;
use Illuminate\Support\Carbon;
use Illuminate\Support\Facades\Artisan;
use Illuminate\Support\Facades\DB;
use Livewire\Livewire;

beforeEach(function (): void {
    Filament::setCurrentPanel(Filament::getPanel('admin'));
});

test('scheduler heartbeat is registered every minute before transfer pruning', function () {
    expect(Artisan::call('schedule:list'))->toBe(0);

    $schedule = Artisan::output();

    expect($schedule)->toMatch('/\*\s+\*\s+\*\s+\*\s+\*\s+filebeam:scheduler-heartbeat/')
        ->and($schedule)->toContain('php artisan filebeam:prune-transfers')
        ->and(strpos($schedule, 'filebeam:scheduler-heartbeat'))->toBeLessThan(strpos($schedule, 'php artisan filebeam:prune-transfers'));
});

test('daily release checks are scheduled', function () {
    expect(Artisan::call('schedule:list', ['--json' => true]))->toBe(0);

    $releaseCheck = collect(json_decode(Artisan::output(), true, flags: JSON_THROW_ON_ERROR))
        ->first(fn (array $event): bool => $event['command'] === 'filebeam:check-updates');

    expect($releaseCheck)->not->toBeNull()
        ->and($releaseCheck['expression'])->toBe('0 0 * * *');
});

test('scheduler heartbeat is created then atomically updated', function () {
    $this->travelTo(Carbon::parse('2026-09-08 12:00:00', 'UTC'));
    $this->artisan('schedule:run')->assertSuccessful();

    $this->assertDatabaseHas('scheduler_heartbeats', [
        'name' => 'scheduler',
        'last_run_at' => '2026-09-08 12:00:00',
    ]);

    $this->travelTo(Carbon::parse('2026-09-08 12:01:00', 'UTC'));
    $this->artisan('schedule:run')->assertSuccessful();

    expect(DB::table('scheduler_heartbeats')->count())->toBe(1);
    $this->assertDatabaseHas('scheduler_heartbeats', [
        'name' => 'scheduler',
        'last_run_at' => '2026-09-08 12:01:00',
    ]);
});

test('scheduler warning is hidden at the exact one-hour boundary and returns after recovery', function () {
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    $this->actingAs($admin, 'admin');
    $now = Carbon::parse('2026-09-08 12:00:00', 'UTC')->startOfSecond();
    $this->travelTo($now);
    $now = now('UTC')->startOfSecond();

    $component = Livewire::test(SchedulerHeartbeatAlert::class)
        ->assertSee('Scheduler heartbeat is unhealthy');

    DB::table('scheduler_heartbeats')->insert([
        'name' => 'scheduler',
        'last_run_at' => $now->copy()->subHour(),
    ]);

    $component->call('$refresh')
        ->assertDontSee('Scheduler heartbeat is unhealthy');

    $staleAt = $now->copy()->subHour()->subSecond()->startOfSecond();
    DB::table('scheduler_heartbeats')->update(['last_run_at' => $staleAt]);

    $component->call('$refresh')
        ->assertSee('Scheduler heartbeat is unhealthy');

    DB::table('scheduler_heartbeats')->update(['last_run_at' => $now]);

    $component->call('$refresh')
        ->assertDontSee('Scheduler heartbeat is unhealthy');
});

test('scheduler warning is absent from login and visible to staff across panel pages', function () {
    $this->get('/admin/login')
        ->assertOk()
        ->assertDontSee('Scheduler heartbeat is unhealthy');

    $moderator = User::factory()->create(['role' => UserRole::Moderator]);
    $this->actingAs($moderator, 'admin')
        ->get('/admin/transfers')
        ->assertOk()
        ->assertSee('fi-scheduler-heartbeat-alert', false)
        ->assertSee('Scheduler heartbeat is unhealthy');

    $admin = User::factory()->create(['role' => UserRole::Admin]);
    $this->actingAs($admin, 'admin')
        ->get('/admin/transfers')
        ->assertOk()
        ->assertSee('Scheduler heartbeat is unhealthy');
});
