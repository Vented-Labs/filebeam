<?php

declare(strict_types=1);

namespace App\Notifications;

use App\Models\User;
use Illuminate\Bus\Queueable;
use Illuminate\Contracts\Queue\ShouldQueue;
use Illuminate\Notifications\Messages\MailMessage;
use Illuminate\Notifications\Notification;

class InboxTransferCompleted extends Notification implements ShouldQueue
{
    use Queueable;

    public function __construct()
    {
        $this->afterCommit();
    }

    /** @return list<string> */
    public function via(object $notifiable): array
    {
        return [$notifiable instanceof User && in_array($notifiable->notification_channel, ['mail', 'database'], true) ? $notifiable->notification_channel : 'mail'];
    }

    public function toMail(object $notifiable): MailMessage
    {
        $brandName = (string) config('filebeam.branding.name');

        return (new MailMessage)
            ->subject("A transfer is ready in your {$brandName} inbox")
            ->line("A new encrypted transfer is ready in your {$brandName} inbox.")
            ->action('Open inbox', rtrim((string) config('app.url'), '/').route('inbox.index', absolute: false));
    }

    /** @return array<string, string> */
    public function toArray(object $notifiable): array
    {
        return ['message' => 'A new encrypted transfer is ready in your inbox.'];
    }
}
