<?php

declare(strict_types=1);

namespace App\Notifications;

use App\Models\User;
use Illuminate\Bus\Queueable;
use Illuminate\Contracts\Queue\ShouldQueue;
use Illuminate\Notifications\Messages\MailMessage;
use Illuminate\Notifications\Notification;

class FriendshipChanged extends Notification implements ShouldQueue
{
    use Queueable;

    public function __construct(public bool $accepted)
    {
        $this->afterCommit();
    }

    /** @return list<string> */
    public function via(object $notifiable): array
    {
        return [$notifiable instanceof User && $notifiable->notification_channel === 'database' ? 'database' : 'mail'];
    }

    public function toMail(object $notifiable): MailMessage
    {
        $brandName = (string) config('filebeam.branding.name');

        return (new MailMessage)->subject("Your {$brandName} contacts have changed")
            ->line($this->message())->action('Open contacts', rtrim((string) config('app.url'), '/').'/account/contacts');
    }

    /** @return array<string, string> */
    public function toArray(object $notifiable): array
    {
        return ['message' => $this->message()];
    }

    private function message(): string
    {
        return $this->accepted ? 'A friend request has been accepted.' : 'You have a new friend request.';
    }
}
