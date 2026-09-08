<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('plans', function (Blueprint $table) {
            $table->id();
            $table->string('slug')->unique();
            $table->string('name');
            $table->unsignedBigInteger('maximum_transfer_bytes');
            $table->unsignedSmallInteger('maximum_file_count');
            $table->unsignedBigInteger('maximum_note_bytes');
            $table->unsignedInteger('default_file_retention_hours');
            $table->unsignedInteger('maximum_file_retention_hours');
            $table->unsignedInteger('default_note_retention_hours');
            $table->unsignedInteger('maximum_note_retention_hours');
            $table->string('placement_mode')->default('distribute');
            $table->boolean('is_active')->default(true);
            $table->timestamps();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('plans');
    }
};
