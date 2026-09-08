<?php

declare(strict_types=1);

namespace App\Support;

use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;

class SocialPreview
{
    /**
     * @return array{title: string, description: string, url: string, image_url: ?string}|null
     */
    public function for(Request $request): ?array
    {
        $card = match (Route::currentRouteName()) {
            'home' => 'home',
            'receive.show', 'receive.domain' => 'receive',
            'transfers.show' => 'transfer',
            default => null,
        };

        if ($card === null) {
            return null;
        }

        $name = (string) config('filebeam.branding.name');

        return [
            'title' => match ($card) {
                'home' => $name.' | Secure file sharing',
                'receive' => 'Send files securely with '.$name,
                'transfer' => 'Receive files securely with '.$name,
            },
            'description' => match ($card) {
                'home' => 'End-to-end encrypted file sharing with '.$name.'.',
                'receive' => 'Send end-to-end encrypted files to a recipient with '.$name.'.',
                'transfer' => 'Receive an end-to-end encrypted file transfer with '.$name.'.',
            },
            'url' => $request->url(),
            'image_url' => $this->imageUrl($card),
        ];
    }

    private function imageUrl(string $card): ?string
    {
        $override = config('social.image_url');

        if (is_string($override) && $this->isHttpUrl($override)) {
            return $override;
        }

        $path = config('social.manifest_path');

        if (! is_string($path) || ! is_file($path)) {
            return null;
        }

        $manifest = json_decode((string) file_get_contents($path), true);
        $filename = is_array($manifest) ? $manifest[$card] ?? null : null;

        if (! is_string($filename) || basename($filename) !== $filename || ! str_ends_with($filename, '.png')) {
            return null;
        }

        if (! is_file(dirname($path).'/'.$filename)) {
            return null;
        }

        return asset('build/og/'.$filename);
    }

    private function isHttpUrl(string $url): bool
    {
        $parts = parse_url($url);

        return $parts !== false
            && isset($parts['scheme'], $parts['host'])
            && in_array(strtolower($parts['scheme']), ['http', 'https'], true)
            && filter_var($url, FILTER_VALIDATE_URL) !== false;
    }
}
