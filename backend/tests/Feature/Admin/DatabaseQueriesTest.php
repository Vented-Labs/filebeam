<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Filament\Resources\AdminAudits\AdminAuditResource;
use App\Filament\Resources\AdminAudits\Pages\ListAdminAudits;
use App\Filament\Resources\Transfers\TransferResource;
use App\Filament\Resources\Users\UserResource;
use App\Models\AdminAudit;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\User;
use Carbon\CarbonImmutable;
use Filament\Facades\Filament;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Events\QueryExecuted;
use Illuminate\Support\Facades\DB;
use Livewire\Livewire;

test('audit date filters use inclusive app-timezone days', function () {
    $administrator = User::factory()->create(['role' => UserRole::Admin]);
    $day = CarbonImmutable::parse('2026-09-07', config('app.timezone'))->startOfDay();
    $before = AdminAudit::factory()->create(['created_at' => $day->subSecond()]);
    $start = AdminAudit::factory()->create(['created_at' => $day]);
    $end = AdminAudit::factory()->create(['created_at' => $day->addDay()->subSecond()]);
    $after = AdminAudit::factory()->create(['created_at' => $day->addDay()]);

    $this->actingAs($administrator, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListAdminAudits::class)
        ->filterTable('date_range', ['from' => $day->toDateString(), 'until' => $day->toDateString()])
        ->assertCanSeeTableRecords([$start, $end])
        ->assertCanNotSeeTableRecords([$before, $after]);
});

test('audit search matches ASCII case variants on every database engine', function () {
    $administrator = User::factory()->create(['role' => UserRole::Admin]);
    $match = AdminAudit::factory()->create(['action' => 'transfer.takedown_requested']);
    $other = AdminAudit::factory()->create(['action' => 'user.plan_changed']);

    $this->actingAs($administrator, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListAdminAudits::class)
        ->searchTable('TAKEDOWN_REQUESTED')
        ->assertCanSeeTableRecords([$match])
        ->assertCanNotSeeTableRecords([$other]);
});

test('detail resource queries eager load dotted relations without query growth', function () {
    $administrator = User::factory()->create(['role' => UserRole::Admin]);
    $plans = Plan::factory()->count(2)->create();
    $users = User::factory()->count(2)->sequence(
        ['plan_id' => $plans[0]->id],
        ['plan_id' => $plans[1]->id],
    )->create();
    $transfers = Transfer::factory()->count(2)->sequence(
        ['owner_id' => $users[0]->id, 'plan_id' => $plans[0]->id],
        ['owner_id' => $users[1]->id, 'plan_id' => $plans[1]->id],
    )->create();
    $audits = AdminAudit::factory()->count(2)->sequence(
        ['actor_id' => $users[0]->id],
        ['actor_id' => $users[1]->id],
    )->create();
    $userKeys = $users->modelKeys();
    $transferKeys = $transfers->modelKeys();
    $auditKeys = $audits->modelKeys();

    $this->actingAs($administrator, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    $connection = DB::connection();
    $eventDispatcher = $connection->getEventDispatcher();
    $connection->setEventDispatcher(clone $eventDispatcher);
    $queries = [];
    DB::listen(function (QueryExecuted $query) use (&$queries): void {
        $queries[] = $query;
    });
    $countQueries = function (Closure $callback) use (&$queries): int {
        $queries = [];
        $callback();

        return count($queries);
    };
    $wasPreventingLazyLoading = Model::preventsLazyLoading();
    Model::preventLazyLoading();

    try {
        $singleUserQueries = $countQueries(fn (): mixed => UserResource::getEloquentQuery()->whereKey($users[0])->get()->each(fn (User $user): string => $user->plan->name));
        $multipleUserQueries = $countQueries(fn (): mixed => UserResource::getEloquentQuery()->whereKey($userKeys)->get()->each(fn (User $user): string => $user->plan->name));
        $singleTransferQueries = $countQueries(fn (): mixed => TransferResource::getEloquentQuery()->whereKey($transfers[0])->get()->each(fn (Transfer $transfer): string => $transfer->owner->email.$transfer->plan->name));
        $multipleTransferQueries = $countQueries(fn (): mixed => TransferResource::getEloquentQuery()->whereKey($transferKeys)->get()->each(fn (Transfer $transfer): string => $transfer->owner->email.$transfer->plan->name));
        $singleAuditQueries = $countQueries(fn (): mixed => AdminAuditResource::getEloquentQuery()->whereKey($audits[0])->get()->each(fn (AdminAudit $audit): string => $audit->actor->email));
        $multipleAuditQueries = $countQueries(fn (): mixed => AdminAuditResource::getEloquentQuery()->whereKey($auditKeys)->get()->each(fn (AdminAudit $audit): string => $audit->actor->email));

        expect($multipleUserQueries)->toBe($singleUserQueries)
            ->and($multipleTransferQueries)->toBe($singleTransferQueries)
            ->and($multipleAuditQueries)->toBe($singleAuditQueries);
    } finally {
        Model::preventLazyLoading($wasPreventingLazyLoading);
        $connection->setEventDispatcher($eventDispatcher);
    }
});
