<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('transfer_chunks', function (Blueprint $table) {
            $table->id();
            $table->foreignUlid('transfer_item_id')->constrained()->cascadeOnDelete();
            $table->unsignedInteger('position');
            $table->unsignedInteger('ciphertext_bytes');
            $table->char('checksum', 64);
            $table->timestamps();

            $table->unique(['transfer_item_id', 'position']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('transfer_chunks');
    }
};
