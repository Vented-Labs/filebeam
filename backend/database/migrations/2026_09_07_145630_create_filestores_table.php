<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('filestores', function (Blueprint $table) {
            $table->id();
            $table->string('name');
            $table->string('source');
            $table->string('disk_name')->nullable()->unique();
            $table->string('driver')->nullable();
            $table->text('configuration')->nullable();
            $table->boolean('placement_enabled')->default(true);
            $table->timestamps();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('filestores');
    }
};
