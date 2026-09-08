<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('account_key_bundles', function (Blueprint $table) {
            $table->id();
            $table->foreignId('user_id')->constrained()->cascadeOnDelete();
            $table->unsignedInteger('version');
            $table->text('public_key');
            $table->char('fingerprint', 64);
            $table->string('custody_mode', 16)->default('password');
            $table->longText('encrypted_private_key')->nullable();
            $table->boolean('is_active')->default(true);
            $table->timestamp('retired_at')->nullable();
            $table->timestamps();

            $table->unique(['user_id', 'version']);
            $table->index(['user_id', 'is_active']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('account_key_bundles');
    }
};
