<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('device_push_tokens', function (Blueprint $table): void {
            $table->id();
            $table->foreignUlid('device_id')->constrained('devices')->cascadeOnDelete();
            $table->string('provider', 8);
            $table->string('token_hash', 64);
            $table->text('token_ciphertext');
            $table->timestamp('revoked_at')->nullable();
            $table->timestamps();
            $table->unique(['provider', 'token_hash']);
            $table->index(['device_id', 'provider', 'revoked_at']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('device_push_tokens');
    }
};
