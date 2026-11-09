<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('changelog_releases', function (Blueprint $table): void {
            $table->id();
            $table->string('source', 160);
            $table->unsignedBigInteger('upstream_id');
            $table->string('tag', 255);
            $table->string('name', 255)->nullable();
            $table->longText('body')->nullable();
            $table->string('author', 39)->nullable();
            $table->string('html_url', 500);
            $table->boolean('prerelease')->default(false);
            $table->boolean('visible')->default(true);
            $table->timestamp('published_at')->nullable();
            $table->char('content_hash', 64);
            $table->string('renderer_version', 64)->nullable();
            $table->longText('document')->nullable();
            $table->longText('search_text')->nullable();
            $table->unsignedSmallInteger('missing_scans')->default(0);
            $table->unsignedBigInteger('seen_generation')->default(0);
            $table->timestamps();
            $table->unique(['source', 'upstream_id']);
            $table->index(['source', 'visible', 'published_at', 'upstream_id'], 'changelog_releases_feed');
            $table->index(['source', 'tag']);
        });

        Schema::create('changelog_sync_states', function (Blueprint $table): void {
            $table->string('source', 160)->primary();
            $table->unsignedBigInteger('generation')->default(0);
            $table->timestamp('last_attempt_at')->nullable();
            $table->timestamp('last_success_at')->nullable();
            $table->timestamp('last_full_success_at')->nullable();
            $table->unsignedBigInteger('latest_upstream_id')->nullable();
            $table->timestamp('next_attempt_at')->nullable();
            $table->unsignedSmallInteger('failures')->default(0);
            $table->string('last_error', 64)->nullable();
            $table->longText('pages')->nullable();
            $table->timestamps();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('changelog_sync_states');
        Schema::dropIfExists('changelog_releases');
    }
};
