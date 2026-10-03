<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::table('users', function (Blueprint $table): void {
            $table->string('receiving_policy', 24)->default('anyone');
            $table->boolean('auto_download_friends')->default(false);
            $table->unsignedBigInteger('receiving_revision')->default(1);
        });
        Schema::create('friendships', function (Blueprint $table): void {
            $table->id();
            $table->foreignId('lower_user_id')->constrained('users')->cascadeOnDelete();
            $table->foreignId('upper_user_id')->constrained('users')->cascadeOnDelete();
            $table->foreignId('requester_id')->constrained('users')->cascadeOnDelete();
            $table->timestamp('accepted_at')->nullable();
            $table->timestamps();
            $table->unique(['lower_user_id', 'upper_user_id']);
        });
        Schema::create('contact_preferences', function (Blueprint $table): void {
            $table->id();
            $table->foreignId('friendship_id')->constrained()->cascadeOnDelete();
            $table->foreignId('user_id')->constrained()->cascadeOnDelete();
            $table->boolean('can_send')->nullable();
            $table->boolean('auto_download')->nullable();
            $table->timestamps();
            $table->unique(['friendship_id', 'user_id']);
        });
        Schema::create('contact_blocks', function (Blueprint $table): void {
            $table->id();
            $table->foreignId('user_id')->constrained()->cascadeOnDelete();
            $table->foreignId('blocked_user_id')->constrained('users')->cascadeOnDelete();
            $table->timestamps();
            $table->unique(['user_id', 'blocked_user_id']);
        });
        Schema::table('transfers', function (Blueprint $table): void {
            $table->boolean('sender_authenticated')->default(false);
        });
        DB::table('transfers')->whereNotNull('owner_id')->update(['sender_authenticated' => true]);
    }

    public function down(): void
    {
        Schema::table('transfers', fn (Blueprint $table) => $table->dropColumn('sender_authenticated'));
        Schema::dropIfExists('contact_blocks');
        Schema::dropIfExists('contact_preferences');
        Schema::dropIfExists('friendships');
        Schema::table('users', fn (Blueprint $table) => $table->dropColumn(['receiving_policy', 'auto_download_friends', 'receiving_revision']));
    }
};
