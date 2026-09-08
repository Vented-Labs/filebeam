<?php

declare(strict_types=1);

namespace App\Services;

class FilebeamUrlGenerator
{
    public function transfer(string $transferId): string
    {
        return rtrim((string) config('app.url'), '/').'/'.$transferId;
    }

    public function profile(string $username): string
    {
        $usernameDomain = config('filebeam.username_domain');

        if (is_string($usernameDomain) && $usernameDomain !== '') {
            return 'https://'.$usernameDomain.'/'.$username;
        }

        return rtrim((string) config('app.url'), '/').'/u/'.$username;
    }
}
