<?php

declare(strict_types=1);

namespace App\Filament\Pages;

use Filament\Auth\Pages\EditProfile;
use Filament\Schemas\Schema;

class StaffProfile extends EditProfile
{
    public function form(Schema $schema): Schema
    {
        return $schema->components([
            $this->getPasswordFormComponent(),
            $this->getPasswordConfirmationFormComponent(),
            $this->getCurrentPasswordFormComponent(),
        ]);
    }
}
