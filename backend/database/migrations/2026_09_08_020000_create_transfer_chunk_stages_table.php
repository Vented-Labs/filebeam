<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('transfer_chunk_stages', function (Blueprint $table) {
            $table->uuid('id')->primary();
            $table->ulid('transfer_id')->index();
            $table->ulid('transfer_item_id')->index();
            $table->unsignedInteger('position');
            $table->unsignedInteger('ciphertext_bytes');
            $table->char('checksum', 64);
            $table->unsignedInteger('offset')->default(0);
            $table->json('parts')->nullable();
            $table->string('state', 16)->default('receiving');
            $table->timestamp('expires_at')->index();
            $table->timestamp('released_at')->nullable()->index();
            $table->timestamps();

            $table->unique(['transfer_item_id', 'position', 'id']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('transfer_chunk_stages');
    }
};
