<?php

declare(strict_types=1);

use App\Models\User;
use App\Models\UserInvitation;
use App\Notifications\InboxTransferCompleted;
use App\Notifications\UserInvitationNotification;
use Illuminate\Auth\Notifications\ResetPassword;
use Illuminate\Auth\Notifications\VerifyEmail;
use Illuminate\Mail\Markdown;
use Illuminate\Notifications\Messages\MailMessage;

test('notification emails share branded html and text templates', function (): void {
    config()->set('filebeam.branding.name', 'Acme Vault');
    config()->set('filebeam.branding.logo_url', null);
    config()->set('filebeam.branding.copyright_holder', 'Acme, Inc.');
    config()->set('filebeam.branding.copyright_year', 2031);

    $user = User::factory()->unverified()->create();
    $invitation = UserInvitation::factory()->create();
    $messages = [
        (new VerifyEmail)->toMail($user),
        (new ResetPassword('reset-token'))->toMail($user),
        (new UserInvitationNotification('invitation-token'))->toMail($invitation),
        (new InboxTransferCompleted)->toMail($user),
    ];

    foreach ($messages as $message) {
        expect($message)->toBeInstanceOf(MailMessage::class);

        $html = (string) $message->render();
        $text = (string) app(Markdown::class)->renderText($message->markdown, $message->data());

        expect($html)
            ->toContain('<title>Acme Vault</title>')
            ->toContain('Acme Vault')
            ->toContain('Acme, Inc.')
            ->toContain('background-color: #7c3aed')
            ->toContain('brand/filebeam-mark-email.png')
            ->not->toContain('Laravel Logo')
            ->and($text)
            ->toContain('Acme Vault: '.config('app.url'))
            ->toContain('© 2031 Acme, Inc.')
            ->toContain((string) $message->actionUrl);
    }
});

test('configured email-safe logos are absolute and svg logos fall back to text', function (): void {
    config()->set('filebeam.branding.name', 'Acme Vault');
    $user = User::factory()->create();

    config()->set('filebeam.branding.logo_url', '/images/acme.png');
    $htmlWithRasterLogo = (string) (new InboxTransferCompleted)->toMail($user)->render();

    expect($htmlWithRasterLogo)
        ->toContain('src="'.url('/images/acme.png').'"')
        ->toContain('Acme Vault');

    config()->set('filebeam.branding.logo_url', '/images/acme.svg');
    $htmlWithSvgLogo = (string) (new InboxTransferCompleted)->toMail($user)->render();

    expect($htmlWithSvgLogo)
        ->toContain('Acme Vault')
        ->not->toContain('acme.svg')
        ->not->toContain('filebeam-mark-email.png');
});

test('application notification copy uses configured branding and keeps action urls', function (): void {
    config()->set('filebeam.branding.name', 'Acme Vault');
    $user = User::factory()->create();
    $invitation = UserInvitation::factory()->create();

    $inboxMessage = (new InboxTransferCompleted)->toMail($user);
    $invitationMessage = (new UserInvitationNotification('single-use-token'))->toMail($invitation);

    expect($inboxMessage->subject)
        ->toBe('A transfer is ready in your Acme Vault inbox')
        ->and($inboxMessage->introLines)->toContain('A new encrypted transfer is ready in your Acme Vault inbox.')
        ->and($inboxMessage->actionUrl)->toBe(rtrim((string) config('app.url'), '/').route('inbox.index', absolute: false))
        ->and($invitationMessage->subject)->toBe('You have been invited to Acme Vault')
        ->and($invitationMessage->greeting)->toBe('You have been invited to Acme Vault.')
        ->and($invitationMessage->introLines)->toContain('Use this link to create your account. It expires in 72 hours and can be used once.')
        ->and($invitationMessage->actionUrl)->toBe(route('invitations.accept', ['token' => 'single-use-token']));
});
