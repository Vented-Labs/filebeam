<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('device_credential_rotations', function (Blueprint $table): void {
            $table->id();
            $table->foreignId('device_credential_id')->constrained()->cascadeOnDelete();
            $table->string('idempotency_key_hash', 64)->unique();
            $table->string('replacement_token_hash', 64)->unique();
            $table->text('replacement_token_ciphertext');
            $table->timestamp('expires_at');
            $table->timestamps();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('device_credential_rotations');
    }
};
