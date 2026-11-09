<?php

declare(strict_types=1);

namespace App\Services;

use App\Models\InstanceSetting;
use Illuminate\Support\Facades\Crypt;

class PushSettings
{
    private const KEY = 'push';

    /**
     * @return array{enabled: bool, apns: array{team_id: string, key_id: string, bundle_id: string, private_key: string, sandbox: bool}|null, fcm: array{project_id: string, client_email: string, private_key: string}|null}
     */
    public function current(): array
    {
        $ciphertext = InstanceSetting::query()->find(self::KEY)?->value;
        if (! is_string($ciphertext) || $ciphertext === '') {
            return ['enabled' => false, 'apns' => null, 'fcm' => null];
        }

        try {
            $value = json_decode(Crypt::decryptString($ciphertext), true, 16, JSON_THROW_ON_ERROR);
        } catch (\Throwable) {
            return ['enabled' => false, 'apns' => null, 'fcm' => null];
        }

        return is_array($value) ? $this->normalize($value) : ['enabled' => false, 'apns' => null, 'fcm' => null];
    }

    /**
     * Call from the instance-settings owner after validating administrator authorization.
     *
     * @param  array<string, mixed>  $settings
     */
    public function save(array $settings): void
    {
        $existing = $this->current();
        foreach (['apns', 'fcm'] as $provider) {
            if (is_array($settings[$provider] ?? null) && ($settings[$provider]['private_key'] ?? null) === '' && $existing[$provider] !== null) {
                $settings[$provider]['private_key'] = $existing[$provider]['private_key'];
            }
        }
        $normalized = $this->normalize($settings);
        if (! $normalized['enabled'] && $normalized['apns'] === null && $normalized['fcm'] === null) {
            InstanceSetting::query()->whereKey(self::KEY)->delete();

            return;
        }
        InstanceSetting::query()->updateOrCreate(['key' => self::KEY], ['value' => Crypt::encryptString(json_encode($normalized, JSON_THROW_ON_ERROR))]);
    }

    /** @return array{enabled: bool, apns: array{team_id: string, key_id: string, bundle_id: string, private_key: string, sandbox: bool}, fcm: array{project_id: string, client_email: string, private_key: string}} */
    public function form(): array
    {
        $current = $this->current();

        return [
            'enabled' => $current['enabled'],
            'apns' => ['team_id' => $current['apns']['team_id'] ?? '', 'key_id' => $current['apns']['key_id'] ?? '', 'bundle_id' => $current['apns']['bundle_id'] ?? '', 'private_key' => '', 'sandbox' => $current['apns']['sandbox'] ?? false],
            'fcm' => ['project_id' => $current['fcm']['project_id'] ?? '', 'client_email' => $current['fcm']['client_email'] ?? '', 'private_key' => ''],
        ];
    }

    /**
     * @param  array<string, mixed>  $value
     * @return array{enabled: bool, apns: array{team_id: string, key_id: string, bundle_id: string, private_key: string, sandbox: bool}|null, fcm: array{project_id: string, client_email: string, private_key: string}|null}
     */
    private function normalize(array $value): array
    {
        $apns = $this->apns($value['apns'] ?? null);
        $fcm = $this->fcm($value['fcm'] ?? null);

        return ['enabled' => ($value['enabled'] ?? false) === true, 'apns' => $apns, 'fcm' => $fcm];
    }

    /** @return array{team_id: string, key_id: string, bundle_id: string, private_key: string, sandbox: bool}|null */
    private function apns(mixed $value): ?array
    {
        if (! is_array($value) || ! is_string($value['team_id'] ?? null) || ! is_string($value['key_id'] ?? null) || ! is_string($value['bundle_id'] ?? null) || ! is_string($value['private_key'] ?? null) || trim($value['team_id']) === '' || trim($value['key_id']) === '' || trim($value['bundle_id']) === '' || trim($value['private_key']) === '') {
            return null;
        }

        return ['team_id' => $value['team_id'], 'key_id' => $value['key_id'], 'bundle_id' => $value['bundle_id'], 'private_key' => $value['private_key'], 'sandbox' => ($value['sandbox'] ?? false) === true];
    }

    /** @return array{project_id: string, client_email: string, private_key: string}|null */
    private function fcm(mixed $value): ?array
    {
        if (! is_array($value) || ! is_string($value['project_id'] ?? null) || ! is_string($value['client_email'] ?? null) || ! is_string($value['private_key'] ?? null) || trim($value['project_id']) === '' || trim($value['client_email']) === '' || trim($value['private_key']) === '') {
            return null;
        }

        return ['project_id' => $value['project_id'], 'client_email' => $value['client_email'], 'private_key' => $value['private_key']];
    }
}
