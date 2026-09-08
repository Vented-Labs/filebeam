<?php

declare(strict_types=1);

namespace App\Models;

use App\Enums\UserRole;
use Database\Factories\UserFactory;
use Filament\Auth\MultiFactor\App\Concerns\InteractsWithAppAuthentication;
use Filament\Auth\MultiFactor\App\Concerns\InteractsWithAppAuthenticationRecovery;
use Filament\Auth\MultiFactor\App\Contracts\HasAppAuthentication;
use Filament\Auth\MultiFactor\App\Contracts\HasAppAuthenticationRecovery;
use Filament\Models\Contracts\FilamentUser;
use Filament\Panel;
use Illuminate\Contracts\Auth\MustVerifyEmail;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Attributes\Hidden;
use Illuminate\Database\Eloquent\Attributes\Scope;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\HasMany;
use Illuminate\Foundation\Auth\User as Authenticatable;
use Illuminate\Notifications\Notifiable;
use Illuminate\Support\Carbon;

/**
 * @property int $id
 * @property string $name
 * @property string $email
 * @property Carbon|null $email_verified_at
 * @property string $password
 * @property string|null $remember_token
 * @property Carbon|null $created_at
 * @property Carbon|null $updated_at
 * @property UserRole $role
 * @property Carbon|null $suspended_at
 * @property string $notification_channel
 */
#[Fillable(['name', 'username', 'normalized_username', 'email', 'password', 'plan_id', 'inbox_enabled', 'notification_channel'])]
#[Hidden(['password', 'remember_token'])]
class User extends Authenticatable implements FilamentUser, HasAppAuthentication, HasAppAuthenticationRecovery, MustVerifyEmail
{
    /** @use HasFactory<UserFactory> */
    use HasFactory, Notifiable;

    use InteractsWithAppAuthentication, InteractsWithAppAuthenticationRecovery;

    /** @var array<string, mixed> */
    protected $attributes = ['role' => 'user'];

    public function canAccessPanel(Panel $panel): bool
    {
        return $panel->getId() === 'admin' && $this->isStaff();
    }

    public function isStaff(): bool
    {
        return in_array($this->role, [UserRole::Moderator, UserRole::Admin], true)
            && $this->hasVerifiedEmail()
            && $this->suspended_at === null;
    }

    public function isAdmin(): bool
    {
        return $this->role === UserRole::Admin && $this->isStaff();
    }

    /** @return BelongsTo<Plan, $this> */
    public function plan(): BelongsTo
    {
        return $this->belongsTo(Plan::class);
    }

    /** @return HasMany<AccountKeyBundle, $this> */
    public function accountKeyBundles(): HasMany
    {
        return $this->hasMany(AccountKeyBundle::class);
    }

    /** @return HasMany<AccountKeyBundle, $this> */
    public function activeAccountKeyBundles(): HasMany
    {
        return $this->accountKeyBundles()->active();
    }

    /** @return HasMany<Transfer, $this> */
    public function ownedTransfers(): HasMany
    {
        return $this->hasMany(Transfer::class, 'owner_id');
    }

    /** @return HasMany<Transfer, $this> */
    public function receivedTransfers(): HasMany
    {
        return $this->hasMany(Transfer::class, 'recipient_id');
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function activeStaff(Builder $query): void
    {
        $query->whereIn('role', [UserRole::Moderator->value, UserRole::Admin->value])
            ->whereNotNull('email_verified_at')
            ->whereNull('suspended_at');
    }

    /** @param Builder<self> $query */
    #[Scope]
    protected function inboxEnabled(Builder $query): void
    {
        $query->where('inbox_enabled', true)
            ->whereNull('suspended_at');
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'email_verified_at' => 'datetime',
            'password' => 'hashed',
            'inbox_enabled' => 'boolean',
            'role' => UserRole::class,
            'suspended_at' => 'datetime',
        ];
    }
}
