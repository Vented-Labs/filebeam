<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use App\Models\User;
use App\Notifications\InboxTransferCompleted;
use App\Support\Branding;
use App\Support\EffectivePlan;
use App\Support\InstanceSettings;
use App\Support\TransportPolicy;
use Illuminate\Http\Request;
use Illuminate\Support\Arr;
use Inertia\Middleware;

class HandleInertiaRequests extends Middleware
{
    protected $rootView = 'app';

    /**
     * Define the props that are shared by default.
     *
     * @see https://inertiajs.com/shared-data
     *
     * @return array<string, mixed>
     */
    public function share(Request $request): array
    {
        $plan = app(EffectivePlan::class)->default();
        $settings = app(InstanceSettings::class)->values([
            'registration',
            'anonymous_uploads',
            'username_routing',
            ...Branding::KEYS,
        ]);
        $branding = app(Branding::class)->resolve($settings);
        $enabled = static fn (string $key): bool => filter_var($settings[$key], FILTER_VALIDATE_BOOLEAN);

        return [
            ...parent::share($request),
            'name' => config('app.name'),
            'auth' => [
                'user' => function () use ($request): ?array {
                    $user = $request->user();

                    if (! $user instanceof User) {
                        return null;
                    }

                    return [...$user->only('id', 'name', 'username', 'email', 'email_verified_at', 'inbox_enabled', 'notification_channel'), 'unread_inbox_notifications' => $user->unreadNotifications()->where('type', InboxTransferCompleted::class)->count()];
                },
            ],
            'flash' => ['status' => fn () => $request->session()->get('status')],
            'branding' => [
                ...config('filebeam.branding'),
                ...Arr::except($branding, ['community_links']),
                'default_logo_url' => asset('brand/filebeam-logo-header.svg'),
                'default_mark_url' => asset('brand/filebeam-mark.svg'),
            ],
            'filebeam' => [
                'main_site_url' => config('app.url'),
                'cli' => config('filebeam.cli'),
                'github_url' => $branding['github_url'],
                'copyright_holder' => $branding['copyright_holder'],
                'copyright_url' => $branding['copyright_url'],
                'community_links' => $branding['community_links'],
                'maximum_transfer_bytes' => $plan->maximum_transfer_bytes ?? config('filebeam.default_plan.maximum_transfer_bytes'),
                'maximum_file_count' => $plan->maximum_file_count ?? config('filebeam.default_plan.maximum_file_count'),
                'maximum_note_bytes' => $plan->maximum_note_bytes ?? config('filebeam.default_plan.maximum_note_bytes'),
                'file_retention_hours' => $plan->default_file_retention_hours ?? config('filebeam.default_plan.file_retention_hours'),
                'note_retention_hours' => $plan->default_note_retention_hours ?? config('filebeam.default_plan.note_retention_hours'),
                'file_retention_options' => $this->retentionOptions(
                    $plan->default_file_retention_hours ?? config('filebeam.default_plan.file_retention_hours'),
                    $plan->maximum_file_retention_hours ?? config('filebeam.default_plan.file_retention_hours'),
                ),
                'note_retention_options' => $this->retentionOptions(
                    $plan->default_note_retention_hours ?? config('filebeam.default_plan.note_retention_hours'),
                    $plan->maximum_note_retention_hours ?? config('filebeam.default_plan.note_retention_hours'),
                ),
                'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
                'upload_concurrency' => config('filebeam.transfers.upload_concurrency'),
                'download_concurrency' => config('filebeam.transfers.download_concurrency'),
                'registration_enabled' => $enabled('registration'),
                'anonymous_uploads_enabled' => $enabled('anonymous_uploads'),
                'username_routing_enabled' => $enabled('username_routing'),
                'transport_policy' => fn (): array => app(TransportPolicy::class)->configuration($request->user() instanceof User ? $request->user() : null),
            ],
        ];
    }

    /** @return list<int> */
    private function retentionOptions(int $defaultHours, int $maximumHours): array
    {
        $options = array_filter(
            [1, 6, 12, 24, 72, 168, 720, 2160, 8760, $defaultHours, $maximumHours],
            fn (int $hours): bool => $hours <= $maximumHours,
        );
        sort($options);

        return array_values(array_unique($options));
    }
}
