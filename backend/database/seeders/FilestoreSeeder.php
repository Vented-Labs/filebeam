<?php

declare(strict_types=1);

namespace Database\Seeders;

use App\Models\Filestore;
use App\Models\Plan;
use App\Support\FilestoreRegistry;
use Illuminate\Database\Seeder;
use Illuminate\Support\Facades\DB;

class FilestoreSeeder extends Seeder
{
    public function run(): void
    {
        $registry = app(FilestoreRegistry::class);
        $environmentDisks = $registry->environmentDisks();

        if ($environmentDisks !== null) {
            foreach ($environmentDisks as $disk) {
                $registry->disk(new Filestore(['source' => 'laravel', 'disk_name' => $disk]));
            }
        }

        DB::transaction(function () use ($registry, $environmentDisks): void {
            if ($environmentDisks === null) {
                $store = Filestore::query()->firstOrCreate(
                    ['disk_name' => 'transfers'],
                    ['name' => 'Transfers', 'source' => 'laravel', 'placement_enabled' => true],
                );
                $registry->disk($store);
            } else {
                foreach ($environmentDisks as $disk) {
                    $store = Filestore::query()->where('disk_name', $disk)->lockForUpdate()->first();

                    if ($store === null) {
                        Filestore::query()->create([
                            'name' => $disk,
                            'source' => 'environment',
                            'disk_name' => $disk,
                            'placement_enabled' => true,
                        ]);
                    } elseif (in_array($store->source, ['environment', 'laravel'], true)) {
                        $store->forceFill(['name' => $disk, 'source' => 'environment', 'placement_enabled' => true])->save();
                    } else {
                        throw new \InvalidArgumentException('An environment disk conflicts with a database filestore.');
                    }
                }
            }

            $this->initializeUnassignedPlans($registry);
        });
    }

    private function initializeUnassignedPlans(FilestoreRegistry $registry): void
    {
        $stores = Filestore::query()
            ->get()
            ->filter(fn (Filestore $store): bool => $registry->placementEnabled($store))
            ->values();

        if ($stores->isEmpty()) {
            return;
        }

        foreach (Plan::query()->get() as $plan) {
            if (DB::table('plan_filestore')->where('plan_id', $plan->getKey())->exists()) {
                continue;
            }

            foreach ($stores as $store) {
                DB::table('plan_filestore')->insert([
                    'plan_id' => $plan->getKey(),
                    'filestore_id' => $store->getKey(),
                    'is_default' => true,
                ]);
            }
        }
    }
}
