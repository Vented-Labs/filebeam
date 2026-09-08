<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('report_notes', function (Blueprint $table) {
            $table->ulid('id')->primary();
            $table->foreignUlid('file_report_id')->constrained()->cascadeOnDelete();
            $table->foreignId('author_id')->nullable()->constrained('users')->nullOnDelete();
            $table->text('body');
            $table->timestamps();

            $table->index(['file_report_id', 'created_at']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('report_notes');
    }
};
