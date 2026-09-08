<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('file_reports', function (Blueprint $table) {
            $table->ulid('id')->primary();
            $table->foreignUlid('transfer_id')->nullable()->constrained()->nullOnDelete();
            $table->char('transfer_identifier', 26)->index();
            $table->foreignId('reporter_id')->nullable()->index()->constrained('users')->nullOnDelete();
            $table->string('reporter_email')->nullable();
            $table->string('category');
            $table->text('description');
            $table->string('status', 16)->default('open');
            $table->foreignId('assigned_to')->nullable()->constrained('users')->nullOnDelete();
            $table->text('resolution')->nullable();
            $table->timestamp('resolved_at')->nullable();
            $table->timestamps();

            $table->index(['status', 'created_at']);
            $table->index(['assigned_to', 'status']);
            $table->index(['transfer_id', 'status']);
            $table->index('created_at');
        });

        Schema::create('admin_audits', function (Blueprint $table) {
            $table->id();
            $table->foreignId('actor_id')->nullable()->constrained('users')->nullOnDelete();
            $table->string('action');
            $table->string('target_type');
            $table->string('target_id');
            $table->text('reason')->nullable();
            $table->json('changes')->nullable();
            $table->timestamps();

            $table->index(['target_type', 'target_id', 'created_at'], 'admin_audits_target_created_index');
            $table->index(['actor_id', 'created_at']);
            $table->index('created_at');
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('admin_audits');
        Schema::dropIfExists('file_reports');
    }
};
