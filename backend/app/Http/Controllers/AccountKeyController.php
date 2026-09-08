<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Models\AccountKeyBundle;
use App\Models\User;
use App\Support\InstanceSettings;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Hash;
use Illuminate\Validation\ValidationException;
use SodiumException;
use Throwable;

class AccountKeyController extends Controller
{
    public function index(Request $request): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);

        return response()
            ->json(['data' => $user->accountKeyBundles()->orderBy('version')->get()->map(fn (AccountKeyBundle $bundle): array => $this->bundle($bundle))], 200, ['Cache-Control' => 'no-store']);
    }

    /** @return array<string, int|string|bool|null> */
    private function bundle(AccountKeyBundle $bundle): array
    {
        return $bundle->only('id', 'user_id', 'version', 'public_key', 'fingerprint', 'custody_mode', 'encrypted_private_key', 'is_active');
    }

    /**
     * @throws Throwable
     */
    public function store(Request $request): JsonResponse
    {
        abort_unless(app(InstanceSettings::class)->boolean('username_routing'), 404);
        $user = $request->user();
        assert($user instanceof User);
        $data = $request->validate([
            'public_key' => ['required', 'string', 'regex:/^[A-Za-z0-9_-]{43}$/'],
            'fingerprint' => ['required', 'string', 'regex:/^[a-f0-9]{64}$/'],
            'custody_mode' => ['required', 'in:password,self'],
            'encrypted_private_key' => ['nullable', 'string', 'max:2048'],
            'current_password' => ['nullable', 'required_if:custody_mode,password', 'string'],
            'replace' => ['nullable', 'boolean'],
        ]);
        $publicKey = base64_decode(strtr($data['public_key'], '-_', '+/'), true);
        if (! is_string($publicKey) || ! $this->base64urlLength($data['public_key'], 32) || ! $this->isValidX25519PublicKey($publicKey)) {
            throw ValidationException::withMessages(['public_key' => 'Invalid account public key.']);
        }
        if (! hash_equals(hash('sha256', $publicKey), $data['fingerprint'])) {
            throw ValidationException::withMessages(['fingerprint' => 'Fingerprint does not match the public key.']);
        }
        if ($data['custody_mode'] === 'password') {
            $this->validateEnvelope($data['encrypted_private_key'] ?? null);
        } elseif (($data['encrypted_private_key'] ?? null) !== null) {
            throw ValidationException::withMessages(['encrypted_private_key' => 'Self custody keys cannot include a private key.']);
        }

        $bundle = DB::transaction(function () use ($user, $data): AccountKeyBundle {
            $lockedUser = User::query()->lockForUpdate()->findOrFail($user->id);
            if ($data['custody_mode'] === 'password' && (! is_string($data['current_password'] ?? null) || ! Hash::check($data['current_password'], $lockedUser->password))) {
                throw ValidationException::withMessages(['current_password' => 'The current password is incorrect.']);
            }
            $bundles = $lockedUser->accountKeyBundles()->lockForUpdate()->get();
            $active = $bundles->where('is_active', true);
            if ($active->isNotEmpty() && ! ($data['replace'] ?? false)) {
                throw ValidationException::withMessages(['replace' => 'Explicit replacement is required while a key is active.']);
            }
            $lockedUser->accountKeyBundles()->active()->update(['is_active' => false, 'retired_at' => now()]);
            $lockedUser->update(['inbox_enabled' => true]);

            return $lockedUser->accountKeyBundles()->create([
                'version' => ((int) $bundles->max('version')) + 1,
                'public_key' => $data['public_key'],
                'fingerprint' => $data['fingerprint'],
                'custody_mode' => $data['custody_mode'],
                'encrypted_private_key' => $data['encrypted_private_key'] ?? null,
                'is_active' => true,
            ]);
        });

        return response()->json(['data' => $this->bundle($bundle)], 201, ['Cache-Control' => 'no-store']);
    }

    private function base64urlLength(mixed $value, int $length): bool
    {
        $decoded = is_string($value) && preg_match('/^[A-Za-z0-9_-]+$/', $value) === 1
            ? base64_decode(strtr($value, '-_', '+/'), true) : false;

        return is_string($decoded)
            && strlen($decoded) === $length
            && $decoded
                |> base64_encode(...)
                |> (fn ($x) => strtr($x, '+/', '-_'))
                |> (fn ($x) => rtrim($x, '='))
                |> (fn ($x) => hash_equals($x, $value));
    }

    private function isValidX25519PublicKey(string $publicKey): bool
    {
        try {
            sodium_crypto_scalarmult(str_repeat("\x01", 32), $publicKey);
        } catch (SodiumException) {
            return false;
        }

        return true;
    }

    private function validateEnvelope(?string $envelope): void
    {
        $decoded = is_string($envelope) ? json_decode($envelope, true) : null;
        if (! is_array($decoded)
            || array_diff(array_keys($decoded), ['v', 'kdf', 'salt', 'nonce_prefix', 'ciphertext']) !== []
            || count($decoded) !== 5
            || ($decoded['v'] ?? null) !== 1
            || ! is_array($decoded['kdf'] ?? null)
            || array_diff(array_keys($decoded['kdf']), ['name', 'memory_kib', 'iterations', 'parallelism']) !== []
            || count($decoded['kdf']) !== 4
            || ($decoded['kdf']['name'] ?? null) !== 'argon2id'
            || ($decoded['kdf']['memory_kib'] ?? null) !== 65536
            || ($decoded['kdf']['iterations'] ?? null) !== 3
            || ($decoded['kdf']['parallelism'] ?? null) !== 1
            || ! $this->base64urlLength($decoded['salt'] ?? null, 16)
            || ! $this->base64urlLength($decoded['nonce_prefix'] ?? null, 16)
            || ! $this->base64urlLength($decoded['ciphertext'] ?? null, 48)) {
            throw ValidationException::withMessages(['encrypted_private_key' => 'Invalid private key envelope.']);
        }
    }

    /**
     * @throws Throwable
     */
    public function update(Request $request, AccountKeyBundle $bundle): JsonResponse
    {
        abort_unless(app(InstanceSettings::class)->boolean('username_routing'), 404);
        abort_unless($bundle->user_id === $request->user()?->id, 404);
        $data = $request->validate(['is_active' => ['required', 'boolean']]);

        DB::transaction(function () use ($bundle, $data, $request): void {
            $lockedUser = User::query()->lockForUpdate()->findOrFail($request->user()->id);
            $bundles = $lockedUser->accountKeyBundles()->lockForUpdate()->get();
            $locked = $bundles->firstWhere('id', $bundle->id);
            abort_unless($locked instanceof AccountKeyBundle, 404);
            if ($data['is_active']) {
                $lockedUser->accountKeyBundles()->active()->where('id', '!=', $locked->id)->update(['is_active' => false, 'retired_at' => now()]);
                $locked->update(['is_active' => true, 'retired_at' => null]);
            } else {
                $locked->update(['is_active' => false, 'retired_at' => now()]);
                if (! $lockedUser->accountKeyBundles()->active()->exists()) {
                    $lockedUser->update(['inbox_enabled' => false]);
                }
            }
        });

        return response()->json(['data' => $this->bundle($bundle->refresh())], 200, ['Cache-Control' => 'no-store']);
    }
}
