<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('transfer_chunk_uploads', function (Blueprint $table) {
            $table->ulid('id')->primary();
            $table->ulid('transfer_id')->index();
            $table->ulid('transfer_item_id')->index();
            $table->unsignedInteger('position');
            $table->foreignId('filestore_id')->constrained()->restrictOnDelete();
            $table->string('storage_path');
            $table->unsignedInteger('ciphertext_bytes');
            $table->char('checksum', 64);
            $table->timestamp('valid_until')->index();
            $table->boolean('is_reaping')->default(false);
            $table->timestamp('cleanup_started_at')->nullable();
            $table->timestamps();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('transfer_chunk_uploads');
    }
};
