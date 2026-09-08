<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('plan_filestore', function (Blueprint $table) {
            $table->foreignId('plan_id')->constrained()->cascadeOnDelete();
            $table->foreignId('filestore_id')->constrained()->restrictOnDelete();
            $table->boolean('is_default')->default(false);

            $table->unique(['plan_id', 'filestore_id']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('plan_filestore');
    }
};
