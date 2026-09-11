<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;

/**
 * instance_settings becomes the single key/value store for admin-editable settings.
 * Its boolean column turns into JSON-encoded text so lists and strings fit, and the
 * transport policy singleton moves in as two keys.
 */
return new class extends Migration
{
    public function up(): void
    {
        Schema::table('instance_settings', function (Blueprint $table) {
            $table->text('payload')->nullable();
        });

        foreach (DB::table('instance_settings')->get(['key', 'value']) as $row) {
            DB::table('instance_settings')->where('key', $row->key)->update([
                'payload' => $row->value === null ? null : json_encode((bool) $row->value, JSON_THROW_ON_ERROR),
            ]);
        }

        Schema::table('instance_settings', function (Blueprint $table) {
            $table->dropColumn('value');
        });
        Schema::table('instance_settings', function (Blueprint $table) {
            $table->renameColumn('payload', 'value');
        });

        if (Schema::hasTable('instance_transport_policies')) {
            $policy = DB::table('instance_transport_policies')->where('id', 1)->first(['enabled_drivers', 'default_driver']);

            if ($policy !== null) {
                $enabledDrivers = is_string($policy->enabled_drivers) ? json_decode($policy->enabled_drivers, true, 512, JSON_THROW_ON_ERROR) : $policy->enabled_drivers;
                // Every install has this row; only a policy that was actually changed becomes a database override.
                $defaults = ['enabled_drivers' => ['http'], 'default_driver' => 'http'];
                $policyValues = ['enabled_drivers' => $enabledDrivers, 'default_driver' => $policy->default_driver];

                foreach ($policyValues === $defaults ? [] : $policyValues as $key => $value) {
                    if (! DB::table('instance_settings')->where('key', $key)->exists()) {
                        DB::table('instance_settings')->insert([
                            'key' => $key,
                            'value' => json_encode($value, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES),
                            'created_at' => now(),
                            'updated_at' => now(),
                        ]);
                    }
                }
            }

            Schema::drop('instance_transport_policies');
        }
    }

    public function down(): void
    {
        Schema::create('instance_transport_policies', function (Blueprint $table) {
            $table->unsignedTinyInteger('id')->primary();
            $table->json('enabled_drivers');
            $table->string('default_driver');
            $table->timestamps();
        });

        $settings = DB::table('instance_settings')->get(['key', 'value'])->keyBy('key');
        $decode = static fn (?string $value): mixed => $value === null ? null : json_decode($value, true, 512, JSON_THROW_ON_ERROR);

        DB::table('instance_transport_policies')->insert([
            'id' => 1,
            'enabled_drivers' => json_encode($decode($settings->get('enabled_drivers')?->value) ?? ['http'], JSON_THROW_ON_ERROR),
            'default_driver' => $decode($settings->get('default_driver')?->value) ?? 'http',
            'created_at' => now(),
            'updated_at' => now(),
        ]);

        DB::table('instance_settings')->whereNotIn('key', ['registration', 'anonymous_uploads', 'username_routing'])->delete();

        Schema::table('instance_settings', function (Blueprint $table) {
            $table->boolean('payload')->nullable();
        });

        foreach (DB::table('instance_settings')->get(['key', 'value']) as $row) {
            DB::table('instance_settings')->where('key', $row->key)->update(['payload' => $decode($row->value)]);
        }

        Schema::table('instance_settings', function (Blueprint $table) {
            $table->dropColumn('value');
        });
        Schema::table('instance_settings', function (Blueprint $table) {
            $table->renameColumn('payload', 'value');
        });
    }
};
