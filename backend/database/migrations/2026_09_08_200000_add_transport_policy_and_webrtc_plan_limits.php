<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::table('plans', function (Blueprint $table) {
            $table->unsignedBigInteger('webrtc_maximum_transfer_bytes')->nullable();
            $table->unsignedSmallInteger('webrtc_maximum_file_count')->nullable();
            $table->unsignedBigInteger('webrtc_maximum_note_bytes')->nullable();
        });

        DB::table('plans')->update([
            'webrtc_maximum_transfer_bytes' => DB::raw('maximum_transfer_bytes'),
            'webrtc_maximum_file_count' => DB::raw('maximum_file_count'),
            'webrtc_maximum_note_bytes' => DB::raw('maximum_note_bytes'),
        ]);

        Schema::create('instance_transport_policies', function (Blueprint $table) {
            $table->unsignedTinyInteger('id')->primary();
            $table->json('enabled_drivers');
            $table->string('default_driver');
            $table->timestamps();
        });

        DB::table('instance_transport_policies')->insert([
            'id' => 1,
            'enabled_drivers' => json_encode(['http'], JSON_THROW_ON_ERROR),
            'default_driver' => 'http',
            'created_at' => now(),
            'updated_at' => now(),
        ]);
    }

    public function down(): void
    {
        Schema::dropIfExists('instance_transport_policies');

        Schema::table('plans', function (Blueprint $table) {
            $table->dropColumn([
                'webrtc_maximum_transfer_bytes',
                'webrtc_maximum_file_count',
                'webrtc_maximum_note_bytes',
            ]);
        });
    }
};
