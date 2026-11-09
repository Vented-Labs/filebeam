<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::table('users', function (Blueprint $table): void {
            $table->unsignedBigInteger('device_registry_revision')->default(1);
            $table->unsignedInteger('device_generation')->default(1);
        });
        Schema::create('devices', function (Blueprint $table): void {
            $table->ulid('id')->primary();
            $table->foreignId('user_id')->constrained()->cascadeOnDelete();
            $table->string('display_name', 80);
            $table->string('platform', 32);
            $table->string('public_key', 44);
            $table->unsignedInteger('key_version');
            $table->string('status', 16)->index();
            $table->unsignedInteger('generation');
            $table->json('capabilities')->nullable();
            $table->string('enrollment_challenge_hash', 64)->nullable();
            $table->timestamp('enrollment_expires_at')->nullable();
            $table->string('approval_signature', 88)->nullable();
            $table->ulid('approved_by_device_id')->nullable();
            $table->timestamp('approved_at')->nullable();
            $table->timestamp('credential_issued_at')->nullable();
            $table->timestamp('last_seen_at')->nullable();
            $table->timestamp('presence_expires_at')->nullable();
            $table->timestamp('revoked_at')->nullable();
            $table->timestamps();
            $table->unique(['user_id', 'public_key', 'key_version']);
            $table->index(['user_id', 'generation', 'status']);
        });
        Schema::create('device_credentials', function (Blueprint $table): void {
            $table->id();
            $table->foreignUlid('device_id')->constrained('devices')->cascadeOnDelete();
            $table->string('token_hash', 64)->unique();
            $table->timestamp('expires_at');
            $table->timestamp('last_used_at')->nullable();
            $table->timestamp('revoked_at')->nullable();
            $table->timestamps();
            $table->index(['device_id', 'revoked_at']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('device_credentials');
        Schema::dropIfExists('devices');
        Schema::table('users', function (Blueprint $table): void {
            $table->dropColumn(['device_registry_revision', 'device_generation']);
        });
    }
};
