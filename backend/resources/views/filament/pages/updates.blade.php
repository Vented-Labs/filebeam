<x-filament-panels::page>
    <div style="display: grid; gap: 1.5rem;">
        <x-filament::section>
            <x-slot name="heading">Installed version</x-slot>

            <p>{{ config('version.version') }}@if (config('version.tag')) ({{ config('version.tag') }})@endif</p>
            <p class="text-sm text-gray-500">Distribution: {{ config('version.distribution') }}</p>
        </x-filament::section>

        <x-filament::section>
            <x-slot name="heading">Release status</x-slot>

            <p>Last check: {{ $releaseCheck['checked_at'] ?? 'Never' }}</p>
            @if ($releaseCheck['latest'] ?? null)
                <p class="mt-2">Latest release: {{ $releaseCheck['latest']['tag'] }}</p>
                @if ($releaseCheck['latest']['warning'])
                    <p class="mt-2 text-sm text-warning-600">{{ $releaseCheck['latest']['warning'] }}</p>
                @endif
                @foreach ($releaseCheck['latest']['security_warnings'] as $warning)
                    <p class="mt-2 text-sm text-danger-600">{{ $warning }}</p>
                @endforeach
            @elseif (($releaseCheck['state'] ?? null) === 'current')
                <p class="mt-2">This installation is current.</p>
            @endif
            @if ($releaseCheck['error'] ?? null)
                <p class="mt-2 text-sm text-danger-600">{{ $releaseCheck['error'] }}</p>
            @endif

            <div class="mt-4 flex gap-3">
                <x-filament::button wire:click="checkNow">Check now</x-filament::button>
                @if (($releaseCheck['latest']['upgradeable'] ?? false) && ($availability['available'] ?? false))
                    <x-filament::button color="warning" wire:click="upgradeNow">Upgrade now</x-filament::button>
                @endif
            </div>
        </x-filament::section>

        <x-filament::section>
            <x-slot name="heading">Updater</x-slot>

            <p>Status: {{ $updaterStatus['state'] ?? 'idle' }}</p>
            <p class="mt-2">Cron heartbeat: {{ $updaterHeartbeat['at'] ?? 'Not reported' }}</p>
            @if ($updaterStatus['tag'] ?? null)<p class="mt-2">Release: {{ $updaterStatus['tag'] }}</p>@endif
            @if ($updaterStatus['error'] ?? null)<p class="mt-2 text-sm text-danger-600">{{ $updaterStatus['error'] }}</p>@endif
            @unless ($availability['available'] ?? false)
                @foreach ($availability['reasons'] as $reason)
                    <p class="mt-2 text-sm text-warning-600">{{ $reason }}</p>
                @endforeach
            @endunless
        </x-filament::section>
    </div>
</x-filament-panels::page>
