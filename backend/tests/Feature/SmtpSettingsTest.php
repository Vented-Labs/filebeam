<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Filament\Pages\InstanceSettings as InstanceSettingsPage;
use App\Models\AdminAudit;
use App\Models\User;
use App\Support\InstanceMailManager;
use App\Support\SmtpSettings;
use Filament\Facades\Filament;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Mail\Events\MessageSending;
use Illuminate\Mail\Transport\ArrayTransport;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Event;
use Illuminate\Support\Facades\Mail;
use Illuminate\Validation\ValidationException;
use Livewire\Livewire;
use Symfony\Component\Mailer\Transport\Smtp\EsmtpTransport;

beforeEach(function (): void {
    config()->set('app.key', 'base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=');
    config()->set('smtp', ['managed' => false, 'from_address' => null, 'from_name' => null]);
    config()->set('installation.environment_path', base_path('composer.json'));
});

function smtpInput(array $overrides = []): array
{
    return array_replace([
        'enabled' => true, 'host' => 'smtp.example.test', 'port' => 587, 'security' => 'starttls',
        'username' => 'smtp-user', 'password' => ' smtp-secret ',
        'from_address' => 'files@example.test', 'from_name' => 'Files',
    ], $overrides);
}

test('SMTP configuration is encrypted and audit records and form state omit saved secrets', function (): void {
    $actor = User::factory()->create(['role' => UserRole::Admin]);
    $settings = app(SmtpSettings::class);
    $settings->update($actor, smtpInput());

    $raw = DB::table('instance_settings')->where('key', 'smtp')->value('value');
    expect($raw)->not->toContain('smtp-secret')->not->toContain('smtp-user')
        ->and($settings->stored()['password'])->toBe(' smtp-secret ')
        ->and($settings->form()['password'])->toBe('')
        ->and(AdminAudit::query()->sole()->changes)->toBe(['configured' => ['from' => false, 'to' => true]]);

    $settings->update($actor, smtpInput(['password' => '', 'host' => 'new.example.test']));
    expect($settings->stored()['password'])->toBe(' smtp-secret ');
    $settings->update($actor, smtpInput(['password' => 'replacement']));
    expect($settings->stored()['password'])->toBe('replacement');
    $settings->update($actor, smtpInput(['password' => '', 'clear_password' => true]));
    expect($settings->stored()['password'])->toBeNull();
    $settings->update($actor, ['enabled' => false]);
    expect($settings->stored())->toBeNull()->and($settings->mailerConfiguration())->toBeNull();
});

test('SMTP updates require an administrator and reject environment ownership', function (): void {
    $settings = app(SmtpSettings::class);
    expect(fn () => $settings->update(User::factory()->create(), smtpInput()))->toThrow(AuthorizationException::class);
    $actor = User::factory()->create(['role' => UserRole::Admin]);
    $settings->update($actor, smtpInput());
    config()->set('smtp.managed', true);
    expect(fn () => $settings->update($actor, smtpInput()))->toThrow(ValidationException::class)
        ->and(fn () => $settings->validate(smtpInput()))->toThrow(ValidationException::class)
        ->and($settings->mailerConfiguration())->toBeNull()
        ->and($settings->form()['host'])->toBe('')
        ->and($settings->stored()['host'])->toBe('smtp.example.test');
});

test('SMTP validation accepts unauthenticated relays and honors environment sender fields', function (): void {
    config()->set('smtp.from_address', 'operator@example.test');
    config()->set('smtp.from_name', 'Operator');
    $settings = app(SmtpSettings::class);
    $settings->store($settings->validate(smtpInput(['username' => '', 'password' => '', 'from_address' => 'invalid'])));
    expect($settings->mailerConfiguration()['from'])->toBe(['address' => 'operator@example.test', 'name' => 'Operator'])
        ->and($settings->stored()['password'])->toBeNull();
});

test('SMTP validation rejects malformed configuration', function (array $overrides): void {
    expect(fn () => app(SmtpSettings::class)->validate(smtpInput($overrides)))->toThrow(ValidationException::class);
})->with([
    [['host' => 'smtp://user:password@host']], [['port' => 0]], [['port' => 65536]],
    [['security' => 'invalid']], [['from_address' => 'invalid']], [['from_name' => "Files\r\nBcc: other@example.test"]],
]);

test('mail manager refreshes real SMTP transports and senders on successive uses', function (string $security, string $scheme): void {
    $settings = app(SmtpSettings::class);
    $settings->store($settings->validate(smtpInput(['security' => $security])));
    $manager = app('mail.manager');
    expect($manager)->toBeInstanceOf(InstanceMailManager::class);
    $first = $manager->mailer();
    $transport = $first->getSymfonyTransport();
    expect($transport)->toBeInstanceOf(EsmtpTransport::class)
        ->and((string) $transport)->toStartWith($scheme.'://smtp.example.test:587')
        ->and($transport->getUsername())->toBe('smtp-user')
        ->and($transport->getPassword())->toBe(' smtp-secret ')
        ->and($transport->isAutoTls())->toBe($security !== 'none')
        ->and($transport->isTlsRequired())->toBe($security !== 'none');
    $settings->store($settings->validate(smtpInput(['host' => 'changed.example.test', 'password' => 'changed'])));
    $next = $manager->mailer();
    expect($next)->not->toBe($first)
        ->and((string) $next->getSymfonyTransport())->toContain('changed.example.test')
        ->and($next->getSymfonyTransport()->getPassword())->toBe('changed');
    $settings->store(null);
    expect($manager->mailer()->getSymfonyTransport())->toBeInstanceOf(ArrayTransport::class);
})->with([['starttls', 'smtp'], ['tls', 'smtps'], ['none', 'smtp']]);

test('environment transport never mixes stored credentials or overrides explicit non-SMTP mailers', function (): void {
    app(SmtpSettings::class)->store(app(SmtpSettings::class)->validate(smtpInput()));
    config()->set('smtp.managed', true);
    config()->set('mail.mailers.smtp', ['transport' => 'smtp', 'host' => 'operator.test', 'port' => 2525, 'username' => null, 'password' => null]);
    $transport = Mail::mailer('smtp')->getSymfonyTransport();
    expect((string) $transport)->toContain('operator.test')->and($transport->getUsername())->toBe('')->and($transport->getPassword())->toBe('');
    expect(Mail::mailer()->getSymfonyTransport())->toBeInstanceOf(ArrayTransport::class);
});

test('SMTP sender settings reach outgoing messages and update without restarting the manager', function (): void {
    $settings = app(SmtpSettings::class);
    $settings->store($settings->validate(smtpInput()));
    $senders = [];
    Event::listen(MessageSending::class, function ($event) use (&$senders): bool {
        $senders[] = $event->message->getFrom()[0]->getAddress();

        return false;
    });
    Mail::raw('First', fn ($message) => $message->to('recipient@example.test'));
    $settings->store($settings->validate(smtpInput(['from_address' => 'changed@example.test'])));
    Mail::raw('Second', fn ($message) => $message->to('recipient@example.test'));
    expect($senders)->toBe(['files@example.test', 'changed@example.test']);
});

test('environment locked SMTP controls cannot overwrite stored settings', function (): void {
    Filament::setCurrentPanel(Filament::getPanel('admin'));
    $actor = User::factory()->create(['role' => UserRole::Admin]);
    app(SmtpSettings::class)->update($actor, smtpInput());
    config()->set('smtp.managed', true);
    $this->actingAs($actor, 'admin');
    Livewire::test(InstanceSettingsPage::class)
        ->assertFormFieldIsDisabled('smtp.host')
        ->assertFormFieldIsDisabled('smtp.password')
        ->set('data.smtp.host', 'forged.example.test')->call('save')->assertHasNoFormErrors();
    expect(app(SmtpSettings::class)->stored()['host'])->toBe('smtp.example.test');
});

test('admin SMTP form saves settings without hydrating the saved password', function (): void {
    Filament::setCurrentPanel(Filament::getPanel('admin'));
    $actor = User::factory()->create(['role' => UserRole::Admin]);
    $this->actingAs($actor, 'admin');
    Livewire::test(InstanceSettingsPage::class)
        ->fillForm(['smtp' => smtpInput()])->call('save')->assertHasNoFormErrors()
        ->assertSet('data.smtp.password', '')->assertSet('data.smtp.host', 'smtp.example.test');
    expect(app(SmtpSettings::class)->stored()['password'])->toBe(' smtp-secret ');
    Livewire::test(InstanceSettingsPage::class)->assertSet('data.smtp.password', '');
});

test('invalid SMTP settings report field errors and roll back other admin settings', function (): void {
    Filament::setCurrentPanel(Filament::getPanel('admin'));
    $this->actingAs(User::factory()->create(['role' => UserRole::Admin]), 'admin');
    Livewire::test(InstanceSettingsPage::class)
        ->fillForm(['registration' => '0', 'smtp' => smtpInput(['host' => 'smtp://invalid'])])
        ->call('save')->assertHasFormErrors(['smtp.host']);
    $this->assertDatabaseMissing('instance_settings', ['key' => 'smtp']);
    $this->assertDatabaseMissing('instance_settings', ['key' => 'registration']);
});
