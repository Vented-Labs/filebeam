<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('transfer_key_envelopes', function (Blueprint $table) {
            $table->id();
            $table->foreignUlid('transfer_id')->constrained()->cascadeOnDelete();
            $table->foreignId('account_key_bundle_id')->index()->constrained()->restrictOnDelete();
            $table->string('role', 16);
            $table->longText('encrypted_key');
            $table->timestamps();

            $table->unique(['transfer_id', 'account_key_bundle_id', 'role'], 'transfer_envelope_unique');
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('transfer_key_envelopes');
    }
};
