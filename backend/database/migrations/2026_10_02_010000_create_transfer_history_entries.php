<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::table('transfers', function (Blueprint $table): void {
            $table->string('removal_reason', 24)->nullable();
            $table->timestamp('published_at')->nullable();
        });
        Schema::create('transfer_history_entries', function (Blueprint $table): void {
            $table->ulid('id')->primary();
            $table->foreignId('owner_id')->constrained('users')->cascadeOnDelete();
            $table->string('kind', 16);
            $table->string('delivery', 16);
            $table->string('driver', 16);
            $table->string('status', 24);
            $table->unsignedSmallInteger('item_count');
            $table->unsignedBigInteger('ciphertext_bytes');
            $table->unsignedBigInteger('declared_ciphertext_bytes');
            $table->unsignedInteger('retention_hours');
            $table->boolean('burn_on_read');
            $table->timestamp('created_at');
            $table->timestamp('completed_at')->nullable();
            $table->timestamp('published_at')->nullable();
            $table->timestamp('expires_at');
            $table->timestamp('removed_at');
            $table->timestamp('purge_at')->index();
            $table->index(['owner_id', 'created_at', 'id'], 'transfer_history_owner_date');
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('transfer_history_entries');
        Schema::table('transfers', function (Blueprint $table): void {
            $table->dropColumn(['removal_reason', 'published_at']);
        });
    }
};
