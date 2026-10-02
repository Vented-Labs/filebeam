<?php

declare(strict_types=1);

namespace App\Support;

use Illuminate\Mail\MailManager;

class InstanceMailManager extends MailManager
{
    public function mailer($name = null)
    {
        if ($name === null || $name === 'smtp') {
            $configuration = $this->app->make(SmtpSettings::class)->mailerConfiguration();
            if ($configuration !== null) {
                // Resolve on each use so queue workers and Octane never retain old credentials.
                $mailer = $this->build(['name' => 'smtp', ...$configuration]);
                $mailer->alwaysFrom($configuration['from']['address'], $configuration['from']['name']);

                return $mailer;
            }
        }

        return parent::mailer($name);
    }
}
