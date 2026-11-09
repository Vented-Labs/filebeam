<?php

declare(strict_types=1);

namespace App\Services;

use App\Enums\DeviceStatus;
use App\Models\Device;
use App\Support\DevicePolicy;
use App\Support\WebRtcDescription;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Str;
use Symfony\Component\HttpKernel\Exception\HttpExceptionInterface;

class DevicePairRendezvous
{
    /** @return array{id: string, source_device_id: string, target_device_id: string, offer: array{type: string, sdp: string}|null, answer: array{type: string, sdp: string}|null, phase: string, expires_at: int} */
    public function create(Device $source, Device $target): array
    {
        abort_unless($source->user_id === $target->user_id && $source->generation === $target->generation, 404);
        abort_unless($target->status->value === 'active', 404);
        $id = (string) Str::ulid();
        $ttl = app(DevicePolicy::class)->connectionTtlSeconds();
        $entry = ['id' => $id, 'source_device_id' => $source->id, 'target_device_id' => $target->id, 'offer' => null, 'answer' => null, 'phase' => 'waiting', 'expires_at' => now()->addSeconds($ttl)->getTimestamp()];
        Cache::put($this->key($id), $entry, now()->addSeconds($ttl));
        try {
            Cache::lock($this->inboxKey($target->id).':lock', 5)->block(3, function () use ($target, $id, $entry, $ttl): void {
                $inbox = Cache::get($this->inboxKey($target->id), []);
                $inbox = is_array($inbox) ? array_values(array_filter($inbox, fn (mixed $candidate): bool => is_array($candidate) && is_string($candidate['id'] ?? null) && is_int($candidate['expires_at'] ?? null) && $candidate['expires_at'] > now()->getTimestamp())) : [];
                abort_if(count($inbox) >= 32, 429, 'Inbound connection capacity reached.');
                $inbox[] = ['id' => $id, 'expires_at' => $entry['expires_at']];
                Cache::put($this->inboxKey($target->id), $inbox, now()->addSeconds($ttl));
            });
        } catch (HttpExceptionInterface $exception) {
            Cache::forget($this->key($id));

            throw $exception;
        }

        return $entry;
    }

    /** @param array{type: string, sdp: string} $description */
    public function offer(Device $device, string $id, array $description): void
    {
        $this->mutate($device, $id, function (array &$entry) use ($description, $device): void {
            abort_unless($device->id === $entry['source_device_id'] && $entry['offer'] === null, 409);
            $entry['offer'] = $description;
            $entry['phase'] = 'offered';
        });
    }

    /** @param array{type: string, sdp: string} $description */
    public function answer(Device $device, string $id, array $description): void
    {
        $this->mutate($device, $id, function (array &$entry) use ($description, $device): void {
            abort_unless($device->id === $entry['target_device_id'] && $entry['offer'] !== null && $entry['answer'] === null, 409);
            $entry['answer'] = $description;
            $entry['phase'] = 'answered';
        });
    }

    /** @return array{id: string, source_device_id: string, target_device_id: string, offer: array{type: string, sdp: string}|null, answer: array{type: string, sdp: string}|null, phase: string, expires_at: int} */
    public function show(Device $device, string $id): array
    {
        $entry = $this->entry(Cache::get($this->key($id)));
        abort_unless($this->participant($device, $entry) && $entry['expires_at'] > now()->getTimestamp() && ! Cache::has('filebeam:device-revoked:'.$entry['source_device_id']) && ! Cache::has('filebeam:device-revoked:'.$entry['target_device_id']), 404);
        $this->activeParticipants($entry);

        return $entry;
    }

    /** @return list<array{id: string, source_device_id: string, target_device_id: string, offer: array{type: string, sdp: string}|null, answer: array{type: string, sdp: string}|null, phase: string, expires_at: int}> */
    public function inbound(Device $device): array
    {
        return Cache::lock($this->inboxKey($device->id).':lock', 5)->block(3, function () use ($device): array {
            $inbox = Cache::get($this->inboxKey($device->id), []);
            $inbox = is_array($inbox) ? $inbox : [];
            $current = [];
            foreach ($inbox as $reference) {
                if (! is_array($reference) || ! is_string($reference['id'] ?? null)) {
                    continue;
                }
                try {
                    $current[] = $this->show($device, $reference['id']);
                } catch (HttpExceptionInterface) {
                    // Expired, revoked, and invalid entries are removed from the recipient index.
                }
            }
            Cache::put($this->inboxKey($device->id), array_map(fn (array $entry): array => ['id' => $entry['id'], 'expires_at' => $entry['expires_at']], $current), now()->addSeconds(app(DevicePolicy::class)->connectionTtlSeconds()));

            return $current;
        });
    }

    /** @return array{id: string, source_device_id: string, target_device_id: string, offer: array{type: string, sdp: string}|null, answer: array{type: string, sdp: string}|null, phase: string, expires_at: int} */
    private function entry(mixed $value): array
    {
        if (! is_array($value) || ! is_string($value['id'] ?? null) || ! is_string($value['source_device_id'] ?? null) || ! is_string($value['target_device_id'] ?? null) || ! is_string($value['phase'] ?? null) || ! is_int($value['expires_at'] ?? null)) {
            abort(404);
        }
        $offer = $this->descriptionEntry($value['offer'] ?? null);
        $answer = $this->descriptionEntry($value['answer'] ?? null);
        if (($value['offer'] ?? null) !== null && $offer === null || ($value['answer'] ?? null) !== null && $answer === null) {
            abort(404);
        }

        return ['id' => $value['id'], 'source_device_id' => $value['source_device_id'], 'target_device_id' => $value['target_device_id'], 'offer' => $offer, 'answer' => $answer, 'phase' => $value['phase'], 'expires_at' => $value['expires_at']];
    }

    /** @return array{type: string, sdp: string}|null */
    private function descriptionEntry(mixed $value): ?array
    {
        if ($value === null) {
            return null;
        }
        if (! is_array($value) || ! is_string($value['type'] ?? null) || ! is_string($value['sdp'] ?? null)) {
            return null;
        }

        return ['type' => $value['type'], 'sdp' => $value['sdp']];
    }

    public function end(Device $device, string $id): void
    {
        $this->mutate($device, $id, function (array &$entry): void {
            $entry['phase'] = 'ended';
        });
        Cache::forget($this->key($id));
    }

    public function revoke(Device $device): void
    {
        // Pair entries are short lived and carry no private transfer state.
        Cache::put('filebeam:device-revoked:'.$device->id, true, now()->addMinutes(10));
    }

    /** @return array{type: string, sdp: string} */
    public function description(Request $request, string $type): array
    {
        $this->only($request, ['description']);

        return app(WebRtcDescription::class)->from($request, $type);
    }

    /** @param callable(array<string, mixed>&): void $mutation */
    private function mutate(Device $device, string $id, callable $mutation): void
    {
        Cache::lock($this->key($id).':lock', 5)->block(3, function () use ($device, $id, $mutation): void {
            $entry = $this->show($device, $id);
            abort_unless(! Cache::has('filebeam:device-revoked:'.$entry['source_device_id']) && ! Cache::has('filebeam:device-revoked:'.$entry['target_device_id']), 403);
            $mutation($entry);
            Cache::put($this->key($id), $entry, now()->addSeconds(app(DevicePolicy::class)->connectionTtlSeconds()));
        });
    }

    /** @param array<string, mixed> $entry */
    private function participant(Device $device, array $entry): bool
    {
        return $device->id === ($entry['source_device_id'] ?? null) || $device->id === ($entry['target_device_id'] ?? null);
    }

    /** @param array{source_device_id: string, target_device_id: string} $entry */
    private function activeParticipants(array $entry): void
    {
        $devices = Device::query()->whereKey([$entry['source_device_id'], $entry['target_device_id']])->get()->keyBy('id');
        $source = $devices->get($entry['source_device_id']);
        $target = $devices->get($entry['target_device_id']);
        abort_unless($source instanceof Device && $target instanceof Device && $source->status === DeviceStatus::Active && $target->status === DeviceStatus::Active && $source->user_id === $target->user_id && $source->generation === $target->generation, 404);
    }

    private function key(string $id): string
    {
        return 'filebeam:device-pair:'.$id;
    }

    private function inboxKey(string $deviceId): string
    {
        return 'filebeam:device-inbox:'.$deviceId;
    }

    /** @param list<string> $allowed */
    private function only(Request $request, array $allowed): void
    {
        abort_unless(array_diff(array_keys($request->all()), $allowed) === [], 422, 'Unknown fields are not accepted.');
    }
}
