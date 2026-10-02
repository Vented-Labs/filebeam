<?php

declare(strict_types=1);

namespace App\Support;

use App\Models\AdminAudit;
use App\Models\InstanceSetting;
use App\Models\User;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\Crypt;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;
use Illuminate\Support\Facades\Validator;
use Illuminate\Validation\ValidationException;

class SmtpSettings
{
    public function managed(): bool
    {
        return (bool) config('smtp.managed');
    }

    /** @return array<string, mixed>|null */
    public function stored(): ?array
    {
        $value = InstanceSetting::query()->find('smtp')?->value;

        return is_string($value) ? json_decode(Crypt::decryptString($value), true, 512, JSON_THROW_ON_ERROR) : null;
    }

    /** @return array<string, mixed> */
    public function form(bool $installed = true): array
    {
        $stored = $installed && ! $this->managed() ? $this->stored() : null;

        return [
            'enabled' => $stored !== null,
            'host' => $stored['host'] ?? '',
            'port' => $stored['port'] ?? 587,
            'security' => $stored['security'] ?? 'starttls',
            'username' => $stored['username'] ?? '',
            'password' => '',
            'clear_password' => false,
            'from_address' => config('smtp.from_address') ?? $stored['from_address'] ?? '',
            'from_name' => config('smtp.from_name') ?? $stored['from_name'] ?? config('mail.from.name'),
        ];
    }

    /**
     * @param  array<string, mixed>  $input
     * @param  array<string, mixed>|null  $current
     * @return array<string, mixed>|null
     */
    public function validate(array $input, ?array $current = null): ?array
    {
        $enabled = Validator::make(['smtp' => $input], [
            'smtp' => ['array:enabled,host,port,security,username,password,clear_password,from_address,from_name'],
            'smtp.enabled' => ['required', 'boolean'],
        ])->validate()['smtp']['enabled'];

        if ($this->managed()) {
            if ($enabled) {
                throw ValidationException::withMessages(['smtp.enabled' => 'Mail transport is controlled by the environment.']);
            }

            return null;
        }

        if (! $enabled) {
            return null;
        }

        foreach (['from_address', 'from_name'] as $field) {
            $input[$field] = config('smtp.'.$field) ?? ($input[$field] ?? null);
        }
        $data = Validator::make(['smtp' => $input], [
            'smtp.host' => ['required', 'string', 'max:253', 'regex:/\A[a-zA-Z0-9.\-:\[\]]+\z/'],
            'smtp.port' => ['required', 'integer', 'between:1,65535'],
            'smtp.security' => ['required', 'in:starttls,tls,none'],
            'smtp.username' => ['nullable', 'string', 'max:1024'],
            'smtp.password' => ['nullable', 'string', 'max:4096'],
            'smtp.clear_password' => ['sometimes', 'boolean'],
            'smtp.from_address' => ['required', 'email:rfc', 'max:254'],
            'smtp.from_name' => ['required', 'string', 'max:255', 'not_regex:/[\r\n]/'],
        ])->validate()['smtp'];
        $data['port'] = (int) $data['port'];
        $data['password'] = ($data['clear_password'] ?? false)
            ? null
            : (($data['password'] ?? '') !== '' ? $data['password'] : ($current['password'] ?? null));
        unset($data['clear_password']);

        return $data;
    }

    /** @param array<string, mixed>|null $configuration */
    public function store(?array $configuration): void
    {
        if ($configuration === null) {
            InstanceSetting::query()->whereKey('smtp')->delete();
        } else {
            InstanceSetting::query()->updateOrCreate(['key' => 'smtp'], [
                'value' => Crypt::encryptString(json_encode($configuration, JSON_THROW_ON_ERROR)),
            ]);
        }
    }

    /** @param array<string, mixed> $input */
    public function update(User $actor, array $input): void
    {
        DB::transaction(function () use ($actor, $input): void {
            $actor = User::query()->whereKey($actor->getKey())->lockForUpdate()->firstOrFail();
            if (! $actor->isAdmin()) {
                throw new AuthorizationException;
            }
            if ($this->managed()) {
                throw ValidationException::withMessages(['smtp.enabled' => 'Mail transport is controlled by the environment.']);
            }
            InstanceSetting::query()->whereKey('smtp')->lockForUpdate()->first();
            $current = $this->stored();
            $configuration = $this->validate($input, $current);
            if ($configuration === $current) {
                return;
            }
            $this->store($configuration);
            AdminAudit::query()->create([
                'actor_id' => $actor->getKey(),
                'action' => 'instance_setting.updated',
                'target_type' => InstanceSetting::class,
                'target_id' => 'smtp',
                'changes' => ['configured' => ['from' => $current !== null, 'to' => $configuration !== null]],
            ]);
        });
    }

    /** @return array<string, mixed>|null */
    public function mailerConfiguration(): ?array
    {
        if ($this->managed() || ! Schema::hasTable('instance_settings')) {
            return null;
        }
        $stored = $this->stored();
        if ($stored === null) {
            return null;
        }

        return [
            'transport' => 'smtp',
            'scheme' => $stored['security'] === 'tls' ? 'smtps' : 'smtp',
            'host' => $stored['host'],
            'port' => $stored['port'],
            'username' => $stored['username'] ?? null,
            'password' => $stored['password'] ?? null,
            'auto_tls' => $stored['security'] !== 'none',
            'require_tls' => $stored['security'] !== 'none',
            'timeout' => 15,
            'local_domain' => config('mail.mailers.smtp.local_domain'),
            'from' => [
                'address' => config('smtp.from_address') ?? $stored['from_address'],
                'name' => config('smtp.from_name') ?? $stored['from_name'],
            ],
        ];
    }
}
