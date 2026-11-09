<?php

declare(strict_types=1);

use App\Services\Changelog\ReleaseDocumentRenderer;
use App\Services\Changelog\ReleaseNodeTree;

/** @return array{document: array<string, mixed>, search_text: string} */
function renderRelease(string $markdown, string $tag = 'v1.0.0'): array
{
    return app(ReleaseDocumentRenderer::class)->render('42', $tag, null, $markdown, 'https://github.com/Vented-Labs/filebeam/releases/tag/'.$tag);
}

/** @return list<string> */
function blockTypes(array $rendered): array
{
    return array_map(fn (array $block): string => $block['type'] === 'graphic' ? 'graphic:'.$block['key'] : $block['type'], $rendered['document']['blocks']);
}

/** Serializes node trees back to markup so assertions can read like the rendered page. */
function nodeHtml(array $nodes): string
{
    $html = '';
    foreach ($nodes as $node) {
        if (is_string($node)) {
            $html .= htmlspecialchars($node, ENT_NOQUOTES);

            continue;
        }
        $attributes = '';
        foreach ($node['a'] ?? [] as $name => $value) {
            $attributes .= ' '.$name.'="'.htmlspecialchars($value).'"';
        }
        $html .= '<'.$node['t'].$attributes.'>';
        if (! in_array($node['t'], ['img', 'hr', 'br', 'input'], true)) {
            $html .= nodeHtml($node['c'] ?? []).'</'.$node['t'].'>';
        }
    }

    return $html;
}

function releaseHtml(array $rendered): string
{
    return implode("\n", array_map(fn (array $block): string => nodeHtml($block['nodes'] ?? []), $rendered['document']['blocks']));
}

test('a top-level graphic directive renders once at its position', function (): void {
    $rendered = renderRelease("Intro.\n\n<!-- filebeam:graphic friends-devices -->\n\n## Added\n\n- Friends.");

    expect(blockTypes($rendered))->toBe(['content', 'graphic:friends-devices', 'content'])
        ->and(releaseHtml($rendered))->toContain('class="release-lead"')
        ->and(releaseHtml($rendered))->not->toContain('filebeam:graphic');
});

test('unknown, nested, quoted, and code directives never mount artwork', function (string $markdown): void {
    $rendered = renderRelease($markdown);

    expect(blockTypes($rendered))->not->toContain('graphic:friends-devices')->each->toBe('content');
})->with([
    'unknown key' => "Text.\n\n<!-- filebeam:graphic not-a-graphic -->\n\nMore.",
    'fenced code' => "```\n<!-- filebeam:graphic friends-devices -->\n```",
    'indented code' => '    <!-- filebeam:graphic friends-devices -->',
    'inline code' => 'Use `<!-- filebeam:graphic friends-devices -->` here.',
    'blockquote' => '> <!-- filebeam:graphic friends-devices -->',
    'list item' => '- <!-- filebeam:graphic friends-devices -->',
    'larger html block' => "<div>\n<!-- filebeam:graphic friends-devices -->\n</div>",
    'escaped' => '\\<!-- filebeam:graphic friends-devices -->',
    'extra attributes' => '<!-- filebeam:graphic friends-devices src=x -->',
]);

test('at most four graphics render per release', function (): void {
    $markdown = implode("\n\n", array_fill(0, 6, '<!-- filebeam:graphic theme-presets -->'));

    expect(array_filter(blockTypes(renderRelease($markdown)), fn (string $type): bool => $type !== 'content'))->toHaveCount(4);
});

test('reference links remain valid across graphic positions', function (): void {
    $rendered = renderRelease("See [the docs][docs].\n\n<!-- filebeam:graphic theme-presets -->\n\nAgain [the docs][docs].\n\n[docs]: https://example.com/docs");

    expect(substr_count(releaseHtml($rendered), 'href="https://example.com/docs"'))->toBe(2);
});

test('raw html, scripts, event handlers, and unsafe links are removed', function (): void {
    $html = releaseHtml(renderRelease(implode("\n\n", [
        '<script>alert(1)</script>',
        '<img src=x onerror="alert(1)">',
        '<iframe src="https://example.com"></iframe>',
        '<svg onload="alert(1)"></svg>',
        '[click](javascript:alert(1))',
        '[data](data:text/html;base64,PHNjcmlwdD4=)',
        'Inline <span style="color:red" onclick="x()">styled</span> text and {{ vueExpression }}.',
    ])));

    expect($html)->not->toContain('<script')
        ->and($html)->not->toContain('onerror')
        ->and($html)->not->toContain('<iframe')
        ->and($html)->not->toContain('<svg')
        ->and($html)->not->toContain('onclick')
        ->and($html)->not->toContain('style=')
        ->and($html)->not->toContain('javascript:')
        ->and($html)->not->toContain('data:text/html');
});

test('headings get release-prefixed unique ids and sections group under level two headings', function (): void {
    $rendered = renderRelease("## Added\n\n- One\n\n## Added\n\n- Two\n\n## What's Changed\n\n- Three");
    $html = releaseHtml($rendered);

    expect(array_column($rendered['document']['headings'], 'id'))->toBe(['r42-added', 'r42-added-2', 'r42-whats-changed'])
        ->and($html)->toContain('<section class="release-section release-section--labelled" data-kind="added">')
        ->and($html)->toContain('<section class="release-section" data-kind="whats-changed">');
});

test('repository references become compact chips and planned markers become trailing chips', function (): void {
    $html = releaseHtml(renderRelease(implode("\n", [
        '- **Planned:** Local sharing. [#77](https://github.com/Vented-Labs/filebeam/issues/77)',
        '- Paste support by @macaws in https://github.com/Vented-Labs/filebeam/pull/72',
        '- Fixes #69, not an email like me@example.com.',
        '',
        '**Full Changelog**: https://github.com/Vented-Labs/filebeam/compare/v0.3.2...v0.3.3',
    ])));

    expect($html)->toContain('aria-label="Issue 77 on GitHub"')
        ->and($html)->toContain('href="https://github.com/Vented-Labs/filebeam/issues/77">#77</a>')
        ->and($html)->toContain('href="https://github.com/Vented-Labs/filebeam/pull/72">#72</a>')
        ->and($html)->toContain('href="https://github.com/Vented-Labs/filebeam/issues/69">#69</a>')
        ->and($html)->toContain('href="mailto:me@example.com"')
        ->and($html)->toContain('href="https://github.com/macaws"')
        ->and($html)->not->toContain('href="https://github.com/example"')
        ->and($html)->toContain('<strong class="release-chip">Planned</strong></li>')
        ->and($html)->toContain('>v0.3.2...v0.3.3</a>');
});

test('only allowlisted https raster images load inline', function (): void {
    $html = releaseHtml(renderRelease(implode("\n\n", [
        '![Screenshot](https://github.com/user-attachments/assets/1b2c3d4e-0000-4000-8000-000000000000)',
        '![Remote](https://tracker.example.com/pixel.png)',
        '![Vector](https://github.com/Vented-Labs/filebeam/raw/master/logo.svg)',
    ])));

    expect($html)->toContain('src="https://github.com/user-attachments/assets/1b2c3d4e-0000-4000-8000-000000000000" alt="Screenshot"')
        ->and($html)->toContain('loading="lazy"')
        ->and($html)->toContain('referrerpolicy="no-referrer"')
        ->and($html)->not->toContain('<img src="https://tracker.example.com')
        ->and($html)->toContain('>Image: Remote</a>')
        ->and($html)->toContain('>Image: Vector</a>');
});

test('oversized bodies render a source-linked notice instead of truncated notes', function (): void {
    config(['changelog.budget.body_bytes' => 100]);
    $rendered = renderRelease(str_repeat('Long release notes. ', 20));

    expect($rendered['document']['truncated'])->toBeTrue()
        ->and(releaseHtml($rendered))->toContain('Read them on GitHub');
});

test('search text excludes directives and includes tag, name, and prose', function (): void {
    $rendered = app(ReleaseDocumentRenderer::class)->render('7', 'v0.4.0', 'Friends and devices.', "<!-- filebeam:graphic friends-devices -->\n\nSend to a **friend**.", 'https://github.com/x');

    expect($rendered['search_text'])->toContain('v0.4.0')
        ->and($rendered['search_text'])->toContain('friends and devices.')
        ->and($rendered['search_text'])->toContain('send to a friend')
        ->and($rendered['search_text'])->not->toContain('graphic');
});

test('the node tree keeps only allowlisted elements and attributes', function (): void {
    $nodes = ReleaseNodeTree::fromHtml(implode('', [
        '<p class="release-lead" onclick="x()" style="color:red">Intro <span>kept text</span></p>',
        '<svg><text>vector</text></svg><math><mi>x</mi></math><script>alert(1)</script>',
        '<a href="javascript:alert(1)">unsafe</a> <a href="https://example.com" target="_blank">safe</a>',
        '<img src="http://example.com/a.png" alt="insecure"><img src="https://example.com/b.png" alt="ok" onerror="x()">',
        '<ul><li><input type="checkbox" checked> task</li><li><input type="text" value="x"></li></ul>',
    ]));

    expect(nodeHtml($nodes))->toBe(
        '<p class="release-lead">Intro kept text</p>'
        .'unsafe <a href="https://example.com" target="_blank">safe</a>'
        .'<img src="https://example.com/b.png" alt="ok">'
        .'<ul><li><input type="checkbox" checked="" disabled=""> task</li><li></li></ul>'
    );
});

test('content text flattens nested nodes for summaries', function (): void {
    expect(ReleaseNodeTree::text(ReleaseNodeTree::fromHtml('<p>Send <strong>files</strong> &amp; <a href="#n">notes</a>.</p>')))
        ->toBe('Send files & notes.');
});
