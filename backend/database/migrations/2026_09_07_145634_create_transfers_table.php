<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('transfers', function (Blueprint $table) {
            $table->ulid('id')->primary();
            $table->string('kind', 16);
            $table->string('delivery', 16)->default('link');
            $table->foreignId('owner_id')->nullable()->constrained('users')->nullOnDelete();
            $table->foreignId('recipient_id')->nullable()->constrained('users')->nullOnDelete();
            $table->foreignId('plan_id')->index()->constrained()->restrictOnDelete();
            $table->string('status', 16)->default('pending');
            $table->unsignedSmallInteger('protocol_version')->default(1);
            $table->unsignedInteger('chunk_bytes')->default(24999984);
            $table->unsignedInteger('retention_hours')->default(24);
            $table->string('placement_mode')->default('distribute');
            $table->json('filestore_ids')->nullable();
            $table->boolean('burn_on_read')->default(false);
            $table->longText('encrypted_manifest')->nullable();
            $table->longText('encrypted_descriptor')->nullable();
            $table->unsignedBigInteger('declared_ciphertext_bytes');
            $table->unsignedBigInteger('ciphertext_bytes')->default(0);
            $table->unsignedSmallInteger('item_count');
            $table->char('upload_token_hash', 64);
            $table->char('delete_token_hash', 64);
            $table->char('read_token_hash', 64)->nullable();
            $table->char('monitor_token_hash', 64)->nullable();
            $table->timestamp('completed_at')->nullable();
            $table->timestamp('expires_at');
            $table->timestamps();

            $table->index(['status', 'expires_at']);
            $table->index(['status', 'created_at']);
            $table->index(['status', 'updated_at']);
            $table->index('expires_at');
            $table->index('created_at');
            $table->index(['owner_id', 'created_at']);
            $table->index(['recipient_id', 'created_at']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('transfers');
    }
};
