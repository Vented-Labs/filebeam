<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('transfer_items', function (Blueprint $table) {
            $table->ulid('id')->primary();
            $table->foreignUlid('transfer_id')->constrained()->cascadeOnDelete();
            $table->unsignedSmallInteger('position');
            $table->unsignedInteger('chunk_count');
            $table->unsignedBigInteger('declared_ciphertext_bytes');
            $table->unsignedBigInteger('ciphertext_bytes')->default(0);
            $table->timestamp('completed_at')->nullable();
            $table->timestamps();

            $table->unique(['transfer_id', 'position']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('transfer_items');
    }
};
