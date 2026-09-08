<?php

declare(strict_types=1);

namespace App\Notifications;

use App\Models\UserInvitation;
use Illuminate\Bus\Queueable;
use Illuminate\Contracts\Queue\ShouldQueue;
use Illuminate\Notifications\Messages\MailMessage;
use Illuminate\Notifications\Notification;

class UserInvitationNotification extends Notification implements ShouldQueue
{
    use Queueable;

    public function __construct(private readonly string $token)
    {
        $this->afterCommit();
    }

    /** @return array<int, string> */
    public function via(UserInvitation $notifiable): array
    {
        return ['mail'];
    }

    public function toMail(UserInvitation $notifiable): MailMessage
    {
        $brandName = (string) config('filebeam.branding.name');

        return (new MailMessage)
            ->subject("You have been invited to {$brandName}")
            ->greeting("You have been invited to {$brandName}.")
            ->line('Use this link to create your account. It expires in 72 hours and can be used once.')
            ->action('Accept invitation', route('invitations.accept', ['token' => $this->token]));
    }
}
