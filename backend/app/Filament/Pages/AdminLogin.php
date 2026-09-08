<?php

declare(strict_types=1);

namespace App\Filament\Pages;

use App\Support\AuthIdentifier;
use Filament\Auth\Pages\Login;
use Filament\Forms\Components\TextInput;
use Filament\Schemas\Components\Component;

class AdminLogin extends Login
{
    protected function getEmailFormComponent(): Component
    {
        return TextInput::make('email')
            ->label('Email or username')
            ->required()
            ->maxLength(255)
            ->autocomplete('username')
            ->autofocus();
    }

    /**
     * @param  array<string, mixed>  $data
     * @return array<string, mixed>
     */
    protected function getCredentialsFromFormData(array $data): array
    {
        return [
            ...AuthIdentifier::credentials((string) $data['email']),
            'password' => $data['password'],
        ];
    }
}
