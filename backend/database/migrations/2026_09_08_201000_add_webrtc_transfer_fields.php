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
            $table->string('driver', 16)->default('http');
            $table->char('join_token_hash', 64)->nullable();
            $table->ulid('webrtc_claimed_session_id')->nullable();
            $table->index(['driver', 'status', 'expires_at']);
        });
    }

    public function down(): void
    {
        Schema::table('transfers', function (Blueprint $table): void {
            $table->dropIndex(['driver', 'status', 'expires_at']);
            $table->dropColumn(['driver', 'join_token_hash', 'webrtc_claimed_session_id']);
        });
    }
};
