<?php

declare(strict_types=1);

namespace App\Support\Icons;

use Filament\Support\Contracts\LoadingIndicator;
use Illuminate\View\ComponentAttributeBag;

final class IconsaxLoadingIndicator implements LoadingIndicator
{
    public function toHtml(ComponentAttributeBag $attributes): string
    {
        $attributes = $attributes->merge(['aria-hidden' => 'true'], escape: false);

        return svg(
            'filebeam-loader',
            $attributes->get('class'),
            array_filter($attributes->except('class')->getAttributes(), static fn (mixed $value): bool => $value !== false && $value !== null),
        )->toHtml();
    }
}
