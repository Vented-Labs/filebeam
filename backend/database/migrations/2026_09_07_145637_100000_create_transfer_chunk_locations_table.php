<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('transfer_chunk_locations', function (Blueprint $table) {
            $table->id();
            $table->foreignId('transfer_chunk_id')->constrained()->cascadeOnDelete();
            $table->foreignId('filestore_id')->constrained()->restrictOnDelete();
            $table->string('storage_path');
            $table->unsignedInteger('ciphertext_bytes');
            $table->timestamps();

            $table->unique(['transfer_chunk_id', 'filestore_id']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('transfer_chunk_locations');
    }
};
