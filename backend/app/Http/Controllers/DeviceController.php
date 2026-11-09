<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Enums\DeviceStatus;
use App\Enums\TransferDriver;
use App\Models\Device;
use App\Models\DeviceCredential;
use App\Models\User;
use App\Services\DevicePairRendezvous;
use App\Services\DeviceRegistry;
use App\Services\IceServers;
use App\Services\PushWakeService;
use App\Support\DevicePolicy;
use App\Support\TransportPolicy;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Hash;
use Inertia\Inertia;
use Inertia\Response;

class DeviceController extends Controller
{
    public function page(): Response
    {
        return Inertia::render('Devices');
    }

    public function challenge(Request $request): JsonResponse
    {
        $this->only($request, ['device_id', 'display_name', 'platform', 'public_key', 'key_version', 'capabilities']);
        $data = $request->validate([
            'device_id' => ['required', 'ulid'], 'display_name' => ['required', 'string', 'max:80'], 'platform' => ['required', 'string', 'max:32'],
            'public_key' => ['required', 'string'], 'key_version' => ['required', 'integer', 'min:1'], 'capabilities' => ['nullable', 'array', 'max:16'],
        ]);
        $key = $this->key($data['public_key']);
        $user = $request->user();
        assert($user instanceof User);
        $challenge = random_bytes(32);
        $device = DB::transaction(function () use ($data, $key, $user, $challenge): Device {
            $locked = User::query()->lockForUpdate()->findOrFail($user->id);
            $existing = Device::query()->whereKey($data['device_id'])->lockForUpdate()->first();
            abort_unless($existing === null || $existing->user_id === $locked->id, 404);
            abort_if($existing !== null && ($existing->public_key !== $key || $existing->key_version !== $data['key_version'] || $existing->status !== DeviceStatus::Pending), 409);
            $values = [
                'user_id' => $locked->id, 'display_name' => $data['display_name'], 'platform' => $data['platform'], 'public_key' => $key,
                'key_version' => $data['key_version'], 'status' => DeviceStatus::Pending, 'generation' => $locked->device_generation,
                'capabilities' => $data['capabilities'] ?? null, 'enrollment_challenge_hash' => hash('sha256', $challenge), 'enrollment_expires_at' => now()->addMinutes(10),
            ];
            if ($existing === null) {
                return Device::query()->create(['id' => $data['device_id'], ...$values]);
            }
            $existing->update($values);

            return $existing->refresh();
        }, 3);

        return $this->json(['device' => $this->device($device), 'challenge' => base64_encode($challenge), 'expires_at' => $device->enrollment_expires_at?->toIso8601String(), 'approval_context' => $this->approvalContext($device)], 201);
    }

    public function enroll(Request $request, Device $device, DeviceRegistry $registry): JsonResponse
    {
        $this->only($request, ['challenge', 'signature']);
        $data = $request->validate(['challenge' => ['required', 'string'], 'signature' => ['required', 'string']]);
        $user = $request->user();
        assert($user instanceof User);
        $user->refresh();
        abort_unless($device->user_id === $user->id && $device->status === DeviceStatus::Pending && $device->enrollment_expires_at?->isFuture(), 404);
        $challenge = $this->base64($data['challenge'], SODIUM_CRYPTO_SIGN_BYTES / 2);
        $signature = $this->base64($data['signature'], SODIUM_CRYPTO_SIGN_BYTES);
        abort_unless(hash_equals((string) $device->enrollment_challenge_hash, hash('sha256', $challenge)) && sodium_crypto_sign_verify_detached($signature, "filebeam-device-enrollment-v1\0{$device->id}\0{$user->device_generation}\0".$challenge, $device->public_key), 422, 'Invalid enrollment proof.');
        $hasActive = Device::query()->where('user_id', $user->id)->where('generation', $user->device_generation)->active()->exists();
        if ($hasActive) {
            return $this->json(['device' => $this->device($device->refresh()), 'status' => DeviceStatus::Pending->value], 201);
        }
        $activated = $registry->activate($device);
        $credential = $registry->issueCredential($activated);

        return $this->json(['device' => $this->device($activated), 'credential' => $credential, 'status' => DeviceStatus::Active->value], 201);
    }

    public function approve(Request $request, Device $device, DeviceRegistry $registry): JsonResponse
    {
        $this->only($request, ['approval_signature']);
        $data = $request->validate(['approval_signature' => ['required', 'string']]);
        $approver = $this->current($request);
        abort_unless($device->user_id === $approver->user_id && $device->status === DeviceStatus::Pending, 404);
        $signature = $this->base64($data['approval_signature'], SODIUM_CRYPTO_SIGN_BYTES);
        $message = $this->approvalMessage($device, $approver);
        abort_unless(sodium_crypto_sign_verify_detached($signature, $message, $approver->public_key), 422, 'Invalid approval proof.');
        $activated = $registry->activate($device, $approver, $signature);

        return $this->json(['device' => $this->device($activated)], 201);
    }

    public function credential(Request $request, Device $device, DeviceRegistry $registry): JsonResponse
    {
        $this->only($request, ['challenge', 'signature']);
        $data = $request->validate(['challenge' => ['required', 'string'], 'signature' => ['required', 'string']]);
        $challenge = $this->base64($data['challenge'], SODIUM_CRYPTO_SIGN_BYTES / 2);
        $signature = $this->base64($data['signature'], SODIUM_CRYPTO_SIGN_BYTES);
        abort_unless($device->status === DeviceStatus::Active && $device->enrollment_expires_at?->isFuture() && hash_equals((string) $device->enrollment_challenge_hash, hash('sha256', $challenge)) && sodium_crypto_sign_verify_detached($signature, "filebeam-device-enrollment-v1\0{$device->id}\0{$device->generation}\0".$challenge, $device->public_key), 422, 'Invalid enrollment proof.');

        $approver = $device->approved_by_device_id === null ? null : Device::query()
            ->where('user_id', $device->user_id)
            ->find($device->approved_by_device_id);
        $evidence = null;
        if ($approver !== null && is_string($device->approval_signature) && strlen($device->approval_signature) === SODIUM_CRYPTO_SIGN_BYTES) {
            $evidence = [
                'version' => 2,
                'approver_device_id' => $approver->id,
                'approver_key_version' => $approver->key_version,
                'approver_public_key' => base64_encode($approver->public_key),
                'approval_signature' => base64_encode($device->approval_signature),
            ];
        }

        return $this->json(['credential' => $registry->issueCredential($device), 'approval_evidence' => $evidence], 201);
    }

    public function index(Request $request): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);

        return $this->json(['revision' => $user->device_registry_revision, 'generation' => $user->device_generation, 'devices' => Device::query()->where('user_id', $user->id)->orderByDesc('created_at')->get()->map(fn (Device $device): array => $this->device($device))->all()]);
    }

    public function rename(Request $request, Device $device): JsonResponse
    {
        $this->only($request, ['display_name']);
        $data = $request->validate(['display_name' => ['required', 'string', 'max:80']]);
        $user = $request->user();
        assert($user instanceof User);
        abort_unless($device->user_id === $user->id && $device->status !== DeviceStatus::Revoked, 404);
        $device->update(['display_name' => $data['display_name']]);

        return $this->json(['device' => $this->device($device->refresh())]);
    }

    public function revoke(Request $request, Device $device, DeviceRegistry $registry, DevicePairRendezvous $rendezvous, PushWakeService $push): JsonResponse
    {
        $this->only($request, ['current_password']);
        $data = $request->validate(['current_password' => ['required', 'string']]);
        $user = $request->user();
        assert($user instanceof User);
        abort_unless(Hash::check($data['current_password'], $user->password), 422, 'Recent authentication is required.');
        abort_unless($device->user_id === $user->id, 404);
        $device = $registry->revoke($user, $device);
        $rendezvous->revoke($device);
        Device::query()->where('user_id', $user->id)->active()->whereKeyNot($device->id)->each(fn (Device $other): bool => $push->wakeRegistry($other));

        return $this->json(['device' => $this->device($device), 'revision' => $user->fresh()->device_registry_revision]);
    }

    public function reset(Request $request, DeviceRegistry $registry, PushWakeService $push): JsonResponse
    {
        $this->only($request, ['current_password', 'confirmation']);
        $data = $request->validate(['current_password' => ['required', 'string'], 'confirmation' => ['required', 'in:RESET']]);
        $user = $request->user();
        assert($user instanceof User);
        abort_unless(Hash::check($data['current_password'], $user->password), 422, 'Recent authentication is required.');
        $user = $registry->reset($user);
        Device::query()->where('user_id', $user->id)->active()->each(fn (Device $device): bool => $push->wakeRegistry($device));

        return $this->json(['generation' => $user->device_generation, 'revision' => $user->device_registry_revision]);
    }

    public function sync(Request $request): JsonResponse
    {
        $device = $this->current($request);
        $device->update(['last_seen_at' => now()]);
        $user = $device->user;

        return $this->json(['revision' => $user->device_registry_revision, 'generation' => $user->device_generation, 'offline_freshness_seconds' => app(DevicePolicy::class)->offlineFreshnessSeconds(), 'devices' => Device::query()->where('user_id', $user->id)->get()->map(fn (Device $entry): array => $this->device($entry))->all()]);
    }

    public function rotateCredential(Request $request, DeviceRegistry $registry): JsonResponse
    {
        $this->only($request, []);
        $key = $request->header('Idempotency-Key');
        abort_unless(is_string($key) && preg_match('/\A[\x21-\x7e]{16,128}\z/', $key) === 1, 422, 'A valid Idempotency-Key is required.');
        $credential = $request->attributes->get('device_credential');
        assert($credential instanceof DeviceCredential);

        return $this->json(['credential' => $registry->rotateCredential($credential, $key)], 201);
    }

    public function presence(Request $request): JsonResponse
    {
        $this->only($request, ['ready', 'capabilities']);
        $data = $request->validate(['ready' => ['required', 'boolean'], 'capabilities' => ['nullable', 'array', 'max:16']]);
        $device = $this->current($request);
        $device->update(['last_seen_at' => now(), 'presence_expires_at' => $data['ready'] ? now()->addSeconds(app(DevicePolicy::class)->presenceTtlSeconds()) : now(), 'capabilities' => $data['capabilities'] ?? $device->capabilities]);

        return $this->json(['device' => $this->device($device->refresh())]);
    }

    public function pushToken(Request $request, PushWakeService $push): JsonResponse
    {
        $this->only($request, ['provider', 'token']);
        $data = $request->validate(['provider' => ['required', 'string', 'in:apns,fcm'], 'token' => ['required', 'string', 'max:4096']]);
        $push->register($this->current($request), $data['provider'], $data['token']);

        return $this->json([], 204);
    }

    public function connect(Request $request, DevicePairRendezvous $rendezvous, PushWakeService $push): JsonResponse
    {
        $this->only($request, ['target_device_id']);
        $data = $request->validate(['target_device_id' => ['required', 'ulid']]);
        $source = $this->current($request);
        $target = Device::query()->where('user_id', $source->user_id)->whereKey($data['target_device_id'])->firstOrFail();
        $entry = $rendezvous->create($source, $target);
        $push->wake($target, $entry['id']);

        return $this->json(['connection' => $this->connectionPayload($entry)], 201);
    }

    public function inbound(Request $request, DevicePairRendezvous $rendezvous): JsonResponse
    {
        $this->only($request, []);

        return $this->json(['connections' => array_map($this->connectionPayload(...), $rendezvous->inbound($this->current($request))), 'poll_after_seconds' => 30]);
    }

    public function ice(Request $request, string $connection, DevicePairRendezvous $rendezvous, IceServers $iceServers): JsonResponse
    {
        $this->only($request, []);
        $rendezvous->show($this->current($request), $connection);
        abort_unless(app(TransportPolicy::class)->allows(TransferDriver::WebRtc), 404);

        return $this->json(['ice_servers' => $iceServers->forConnection()]);
    }

    public function offer(Request $request, string $connection, DevicePairRendezvous $rendezvous): JsonResponse
    {
        $rendezvous->offer($this->current($request), $connection, $rendezvous->description($request, 'offer'));

        return $this->json([], 204);
    }

    public function answer(Request $request, string $connection, DevicePairRendezvous $rendezvous): JsonResponse
    {
        $rendezvous->answer($this->current($request), $connection, $rendezvous->description($request, 'answer'));

        return $this->json([], 204);
    }

    public function connection(Request $request, string $connection, DevicePairRendezvous $rendezvous): JsonResponse
    {
        return $this->json(['connection' => $this->connectionPayload($rendezvous->show($this->current($request), $connection))]);
    }

    public function end(Request $request, string $connection, DevicePairRendezvous $rendezvous): JsonResponse
    {
        $rendezvous->end($this->current($request), $connection);

        return $this->json([], 204);
    }

    private function current(Request $request): Device
    {
        $device = $request->attributes->get('device');
        assert($device instanceof Device);

        return $device;
    }

    /** @return array<string, mixed> */
    private function device(Device $device): array
    {
        return ['id' => $device->id, 'display_name' => $device->display_name, 'platform' => $device->platform, 'public_key' => base64_encode($device->public_key), 'key_version' => $device->key_version, 'status' => $device->status->value, 'generation' => $device->generation, 'last_seen_at' => $device->last_seen_at?->toIso8601String(), 'presence_expires_at' => $device->presence_expires_at?->toIso8601String(), 'revoked_at' => $device->revoked_at?->toIso8601String()];
    }

    /**
     * @param  array{id: string, source_device_id: string, target_device_id: string, offer: array{type: string, sdp: string}|null, answer: array{type: string, sdp: string}|null, phase: string, expires_at: int}  $entry
     * @return array{id: string, source_device_id: string, target_device_id: string, offer: array{type: string, sdp: string}|null, answer: array{type: string, sdp: string}|null, phase: string, expires_at: string}
     */
    private function connectionPayload(array $entry): array
    {
        return ['id' => $entry['id'], 'source_device_id' => $entry['source_device_id'], 'target_device_id' => $entry['target_device_id'], 'offer' => $entry['offer'], 'answer' => $entry['answer'], 'phase' => $entry['phase'], 'expires_at' => now()->setTimestamp($entry['expires_at'])->toIso8601String()];
    }

    /** @return non-empty-string */
    private function key(string $value): string
    {
        return $this->base64($value, SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES);
    }

    /** @return non-empty-string */
    private function base64(string $value, int $length): string
    {
        $decoded = base64_decode($value, true);
        abort_unless(is_string($decoded) && strlen($decoded) === $length, 422, 'Invalid binary value.');
        if ($decoded === '') {
            abort(422, 'Invalid binary value.');
        }

        return $decoded[0].substr($decoded, 1);
    }

    /** @return non-empty-string */
    private function approvalMessage(Device $candidate, Device $approver): string
    {
        $expiry = $candidate->enrollment_expires_at;
        $digest = is_string($candidate->enrollment_challenge_hash) ? hex2bin($candidate->enrollment_challenge_hash) : false;
        abort_unless($expiry !== null && is_string($digest) && strlen($digest) === 32, 422, 'Invalid enrollment approval context.');
        $origin = rtrim((string) config('app.url'), '/');
        $deployment = (string) config('filebeam.devices.deployment_id');

        return implode("\0", [
            'filebeam-device-approval-v2', $origin, $deployment, (string) $candidate->user_id, (string) $candidate->generation,
            $candidate->id, (string) $candidate->key_version, $approver->id, (string) $approver->key_version, (string) $expiry->getTimestamp(),
        ])."\0".$digest.$candidate->public_key;
    }

    /** @return array{version: int, instance_origin: string, deployment_id: string, account_id: int, generation: int, candidate_device_id: string, candidate_key_version: int, expires_at: string, challenge_sha256: string} */
    private function approvalContext(Device $candidate): array
    {
        $expiry = $candidate->enrollment_expires_at;
        $digest = is_string($candidate->enrollment_challenge_hash) ? hex2bin($candidate->enrollment_challenge_hash) : false;
        abort_unless($expiry !== null && is_string($digest) && strlen($digest) === 32, 422, 'Invalid enrollment approval context.');

        return ['version' => 2, 'instance_origin' => rtrim((string) config('app.url'), '/'), 'deployment_id' => (string) config('filebeam.devices.deployment_id'), 'account_id' => $candidate->user_id, 'generation' => $candidate->generation, 'candidate_device_id' => $candidate->id, 'candidate_key_version' => $candidate->key_version, 'expires_at' => $expiry->toIso8601String(), 'challenge_sha256' => base64_encode($digest)];
    }

    /** @param list<string> $allowed */
    private function only(Request $request, array $allowed): void
    {
        abort_unless(array_diff(array_keys($request->all()), $allowed) === [], 422, 'Unknown fields are not accepted.');
    }

    /** @param array<string, mixed> $data */
    private function json(array $data, int $status = 200): JsonResponse
    {
        return response()->json(['data' => $data], $status, ['Cache-Control' => 'no-store, private']);
    }
}
