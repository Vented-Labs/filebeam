<?php

declare(strict_types=1);

namespace App\Models;

use Database\Factories\UserInvitationFactory;
use Illuminate\Database\Eloquent\Attributes\Fillable;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Notifications\Notifiable;
use Illuminate\Support\Carbon;

/**
 * @property int $id
 * @property string $email
 * @property string $token_hash
 * @property Carbon $expires_at
 * @property Carbon|null $accepted_at
 */
#[Fillable(['invited_by', 'email', 'token_hash', 'expires_at', 'accepted_at'])]
class UserInvitation extends Model
{
    /** @use HasFactory<UserInvitationFactory> */
    use HasFactory, Notifiable;

    /** @return BelongsTo<User, $this> */
    public function invitedBy(): BelongsTo
    {
        return $this->belongsTo(User::class, 'invited_by');
    }

    public function routeNotificationForMail(): string
    {
        return $this->email;
    }

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'expires_at' => 'datetime',
            'accepted_at' => 'datetime',
        ];
    }
}
