<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Http\Middleware\RequireInstallation;
use App\Models\User;
use App\Support\Icons\FilamentIcons;
use App\Support\Icons\Iconsax;
use Filament\Forms\Components\DateTimePicker;
use Filament\Support\Contracts\LoadingIndicator;
use Illuminate\View\ComponentAttributeBag;

test('maps every installed Filament alias and direct built-in reference to Iconsax', function () {
    $aliases = [];
    $references = [];

    foreach (new RecursiveIteratorIterator(new RecursiveDirectoryIterator(base_path('vendor/filament'))) as $file) {
        if (! str_ends_with($file->getFilename(), '.php')) {
            continue;
        }

        $source = file_get_contents($file->getPathname());

        if ($source === false) {
            continue;
        }

        if (str_ends_with($file->getFilename(), 'IconAlias.php')) {
            preg_match_all("/const\\s+\\w+\\s*=\\s*'([^']+)'/", $source, $matches);
            $aliases = [...$aliases, ...$matches[1]];
        }

        preg_match_all('/(?:\\\\?Filament\\\\Support\\\\Icons\\\\)?Heroicon::([A-Za-z0-9_]+)/', $source, $matches);
        $references = [...$references, ...array_filter($matches[1], static fn (string $reference): bool => $reference !== 'class')];
    }

    expect(array_values(array_diff(array_unique($aliases), array_keys(FilamentIcons::aliases()))))->toBeEmpty('unmapped aliases')
        ->and(array_values(array_diff(array_unique($references), array_keys(FilamentIcons::directReferences()))))->toBeEmpty('unmapped direct references');
});

test('compiles direct Filament built-in references to generated Iconsax names', function () {
    $compiled = FilamentIcons::replaceDirectReferences('Heroicon::OutlinedBars3 \\Filament\\Support\\Icons\\Heroicon::XMark');

    expect($compiled)->toBe("'filebeam-menu' 'filebeam-x'");
    expect(FilamentIcons::replaceDirectReferences($compiled))->toBe($compiled);
});

test('preserves the meaning and direction of Filament controls', function () {
    expect(FilamentIcons::aliases())->toMatchArray([
        'actions::view-action' => 'filebeam-eye',
        'forms::components.text-input.actions.hide-password' => 'filebeam-eye-off',
        'panels::pages.password-reset.request-password-reset.actions.login' => 'filebeam-arrow-left',
        'panels::pages.password-reset.request-password-reset.actions.login.rtl' => 'filebeam-arrow-right',
        'panels::theme-switcher.light-button' => 'filebeam-sun',
        'panels::theme-switcher.dark-button' => 'filebeam-moon',
        'panels::theme-switcher.system-button' => 'filebeam-monitor',
        'tables::actions.filter' => 'filebeam-filter',
        'tables::header-cell.sort-button' => 'filebeam-sort',
        'forms::components.select.actions.edit-option' => 'filebeam-edit',
    ])
        ->and(FilamentIcons::directReferences())->toMatchArray([
            'ArrowLeft' => 'arrow-left',
            'ArrowRight' => 'arrow-right',
            'Bold' => 'bold',
            'MagnifyingGlass' => 'search',
            'Sun' => 'sun',
        ]);
});

test('uses the generated Iconsax loader for Filament loading states', function () {
    $html = app(LoadingIndicator::class)->toHtml(new ComponentAttributeBag(['class' => 'fi-loading-indicator']));

    expect($html)->toContain('fi-loading-indicator')
        ->and($html)->toContain('stroke-width="1.5"')
        ->and($html)->not->toContain('fill-rule="evenodd"');
});

test('uses a generated calendar for enabled date controls and forbids unsupported direct-icon components', function () {
    expect(DateTimePicker::make('date')->getSuffixIcon())->toBe('filebeam-calendar');

    $application = '';

    foreach (new RecursiveIteratorIterator(new RecursiveDirectoryIterator(app_path('Filament'))) as $file) {
        if ($file->isFile()) {
            $application .= file_get_contents($file->getPathname()) ?: '';
        }
    }

    expect($application)->not->toContain('RichEditor::')
        ->not->toContain('Wizard::');
});

test('generates decorative Blade icons with the Iconsax notice', function () {
    foreach (array_keys(FilamentIcons::aliases()) as $alias) {
        $name = str_replace('filebeam-', '', FilamentIcons::aliases()[$alias]);
        $svg = file_get_contents(resource_path("icons/iconsax/{$name}.svg"));

        expect($svg)->toStartWith('<!-- Iconsax Free artwork')
            ->toContain('aria-hidden="true"')
            ->toContain('focusable="false"')
            ->toContain('currentColor');
    }

    expect(Iconsax::render('eye', 32))->toContain('width="32" height="32"')
        ->toContain('currentColor');
    expect(fn () => Iconsax::render('unknown'))->toThrow(InvalidArgumentException::class);
    expect(json_decode(file_get_contents(base_path('../icons/approved.json')), true, flags: JSON_THROW_ON_ERROR)['icons']['arrow-up-right'])->toBe('export-arrow-01');
});

test('admin documents render generated Iconsax icons instead of built-in icon output', function () {
    $this->withoutMiddleware(RequireInstallation::class);
    config()->set('app.key', 'base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=');
    $admin = User::factory()->create(['role' => UserRole::Admin]);

    foreach (['/admin/login', '/admin', '/admin/transfers', '/admin/users'] as $url) {
        $response = $url === '/admin/login' ? $this->get($url) : $this->actingAs($admin, 'admin')->get($url);

        $response->assertOk()
            ->assertDontSee('heroicon-', false)
            ->assertDontSee('fill-rule="evenodd"', false);
    }
});
