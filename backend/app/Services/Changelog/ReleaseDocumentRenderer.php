<?php

declare(strict_types=1);

namespace App\Services\Changelog;

use Illuminate\Support\Facades\Log;
use Illuminate\Support\Str;
use League\CommonMark\Environment\Environment;
use League\CommonMark\Extension\CommonMark\CommonMarkCoreExtension;
use League\CommonMark\Extension\CommonMark\Node\Block\BlockQuote;
use League\CommonMark\Extension\CommonMark\Node\Block\Heading;
use League\CommonMark\Extension\CommonMark\Node\Block\HtmlBlock;
use League\CommonMark\Extension\CommonMark\Node\Block\ListItem;
use League\CommonMark\Extension\CommonMark\Node\Inline\Code;
use League\CommonMark\Extension\CommonMark\Node\Inline\Image;
use League\CommonMark\Extension\CommonMark\Node\Inline\Link;
use League\CommonMark\Extension\CommonMark\Node\Inline\Strong;
use League\CommonMark\Extension\Footnote\FootnoteExtension;
use League\CommonMark\Extension\GithubFlavoredMarkdownExtension;
use League\CommonMark\Node\Block\Document;
use League\CommonMark\Node\Block\Paragraph;
use League\CommonMark\Node\Inline\Text;
use League\CommonMark\Node\Node;
use League\CommonMark\Node\StringContainerHelper;
use League\CommonMark\Parser\MarkdownParser;
use League\CommonMark\Renderer\HtmlRenderer;

/**
 * Renders untrusted GitHub release Markdown into a typed document of content blocks and
 * allowlisted native graphics. Raw HTML is stripped by the parser, output is built from the AST,
 * and content blocks carry an allowlisted node tree rather than markup.
 *
 * @phpstan-import-type ReleaseNode from ReleaseNodeTree
 *
 * @phpstan-type Block array{type: 'content', id: string, nodes: list<ReleaseNode>}|array{type: 'graphic', id: string, key: string}
 * @phpstan-type RenderedDocument array{schema: 2, blocks: list<Block>, headings: list<array{id: string, text: string, level: int}>, truncated: bool}
 */
final class ReleaseDocumentRenderer
{
    public const int Schema = 2;

    /** Bump when rendering or sanitization changes so cached documents are rebuilt. */
    private const int Revision = 3;

    public const array Graphics = ['theme-presets', 'friends-devices'];

    private const int MaxGraphics = 4;

    private const string Directive = '/^<!--[ \t]*filebeam:graphic[ \t]+([a-z0-9][a-z0-9-]{0,40})[ \t]*-->$/';

    /** Section labels short enough to sit in the label column beside their content. */
    private const int CompactLabelLength = 10;

    public function version(): string
    {
        return self::Schema.'.'.self::Revision.'-'.substr(hash('sha256', json_encode([
            self::Graphics,
            config('changelog.repository'),
            config('changelog.image_origins'),
            config('changelog.budget.body_bytes'),
        ], JSON_THROW_ON_ERROR)), 0, 16);
    }

    /** @return array{document: RenderedDocument, search_text: string} */
    public function render(string $key, string $tag, ?string $name, ?string $markdown, string $sourceUrl): array
    {
        $markdown = str_replace(["\r\n", "\r"], "\n", (string) $markdown);
        $prefix = 'r'.preg_replace('/[^a-z0-9]/', '', strtolower($key));

        if (strlen($markdown) > (int) config('changelog.budget.body_bytes')) {
            return [
                'document' => [
                    'schema' => self::Schema,
                    'blocks' => [[
                        'type' => 'content',
                        'id' => $prefix.'-b0',
                        'nodes' => ReleaseNodeTree::fromHtml('<p class="release-notice">These release notes are too long to show here. <a href="'.e($sourceUrl).'" target="_blank" rel="noopener noreferrer nofollow">Read them on GitHub</a>.</p>'),
                    ]],
                    'headings' => [],
                    'truncated' => true,
                ],
                'search_text' => $this->normalize($tag.' '.$name),
            ];
        }

        $environment = new Environment([
            'html_input' => 'strip',
            'allow_unsafe_links' => false,
            'max_nesting_level' => 20,
            'max_delimiters_per_line' => 200,
            'renderer' => ['soft_break' => "\n"],
            'footnote' => [
                'ref_id_prefix' => $prefix.'-fnref-',
                'footnote_id_prefix' => $prefix.'-fn-',
                'container_add_hr' => false,
                'container_class' => 'release-footnotes',
            ],
        ]);
        $environment->addExtension(new CommonMarkCoreExtension);
        $environment->addExtension(new GithubFlavoredMarkdownExtension);
        $environment->addExtension(new FootnoteExtension);

        $document = (new MarkdownParser($environment))->parse($markdown);
        $searchText = $this->decorate($document, $prefix);
        $headings = $this->headings($document, $prefix);
        $renderer = new HtmlRenderer($environment);

        $blocks = [];
        $pending = [];
        $graphics = 0;
        $flush = function () use (&$blocks, &$pending, $renderer, $prefix): void {
            $html = trim($this->sections($pending, $renderer));
            if ($html !== '') {
                $blocks[] = ['type' => 'content', 'id' => $prefix.'-b'.count($blocks), 'nodes' => ReleaseNodeTree::fromHtml($html)];
            }
            $pending = [];
        };

        foreach ($document->children() as $node) {
            $graphic = $node instanceof HtmlBlock ? $this->directive($node) : null;
            if ($graphic === null) {
                $pending[] = $node;

                continue;
            }

            if (! in_array($graphic, self::Graphics, true)) {
                Log::debug('changelog.graphic.unknown', ['release' => $key, 'graphic' => Str::limit($graphic, 40)]);

                continue;
            }

            if (++$graphics > self::MaxGraphics) {
                Log::warning('changelog.graphic.limit', ['release' => $key]);

                continue;
            }

            $flush();
            $blocks[] = ['type' => 'graphic', 'id' => $prefix.'-b'.count($blocks), 'key' => $graphic];
        }
        $flush();

        return [
            'document' => ['schema' => self::Schema, 'blocks' => $blocks, 'headings' => $headings, 'truncated' => false],
            'search_text' => $this->normalize($tag.' '.$name.' '.$searchText),
        ];
    }

    private function directive(HtmlBlock $node): ?string
    {
        return preg_match(self::Directive, trim($node->getLiteral()), $match) === 1 ? $match[1] : null;
    }

    /**
     * Groups each level-two heading with the content that follows it, matching the
     * label-and-notes layout used for Added / Changed / Fixed sections.
     *
     * @param  list<Node>  $nodes
     */
    private function sections(array $nodes, HtmlRenderer $renderer): string
    {
        $html = '';
        $heading = null;
        $content = [];

        foreach ($nodes as $node) {
            if ($node instanceof Heading && $node->getLevel() <= 2) {
                if ($heading !== null) {
                    $html .= $this->section($heading, $content, $renderer);
                }
                [$heading, $content] = [null, []];
                if ($node->getLevel() === 2) {
                    $heading = $node;

                    continue;
                }
            }

            if ($heading !== null) {
                $content[] = $node;
            } else {
                $html .= $renderer->renderNodes([$node])."\n";
            }
        }

        return $heading === null ? $html : $html.$this->section($heading, $content, $renderer);
    }

    /** @param list<Node> $content */
    private function section(Heading $heading, array $content, HtmlRenderer $renderer): string
    {
        $label = StringContainerHelper::getChildText($heading, [Image::class]);
        $compact = mb_strlen($label) <= self::CompactLabelLength ? ' release-section--labelled' : '';

        return '<section class="release-section'.$compact.'" data-kind="'.e(Str::slug($label)).'">'
            .$renderer->renderNodes([$heading])
            .'<div class="release-section__body">'.$renderer->renderNodes($content).'</div></section>'."\n";
    }

    /** @return list<array{id: string, text: string, level: int}> */
    private function headings(Document $document, string $prefix): array
    {
        $headings = [];
        $used = [];
        foreach ($document->iterator() as $node) {
            if (! $node instanceof Heading) {
                continue;
            }
            $text = trim(StringContainerHelper::getChildText($node, [Image::class]));
            $base = $prefix.'-'.(Str::slug($text) ?: 'section');
            $id = $base;
            for ($suffix = 2; isset($used[$id]); $suffix++) {
                $id = $base.'-'.$suffix;
            }
            $used[$id] = true;
            $node->data->set('attributes/id', $id);
            if ($node->getLevel() <= 3 && $text !== '') {
                $headings[] = ['id' => $id, 'text' => Str::limit($text, 80), 'level' => $node->getLevel()];
            }
        }

        return $headings;
    }

    /** Applies link, image, reference, and chip policy in place and returns visible text. */
    private function decorate(Document $document, string $prefix): string
    {
        $repository = strtolower((string) config('changelog.repository'));
        $text = [];
        // The intro is the first paragraph before any section, after optional notices.
        foreach ($document->children() as $child) {
            if ($child instanceof Paragraph) {
                $child->data->set('attributes/class', 'release-lead');
                break;
            }
            if (! $child instanceof BlockQuote) {
                break;
            }
        }

        $nodes = iterator_to_array($document->iterator(), false);
        foreach ($nodes as $node) {
            if ($node instanceof Text || $node instanceof Code) {
                $text[] = $node->getLiteral();
            }

            match (true) {
                $node instanceof Link => $this->link($node, $repository),
                $node instanceof Image => $this->image($node),
                $node instanceof ListItem => $this->planned($node),
                $node instanceof Text => $this->references($node, $repository),
                default => null,
            };
        }

        return implode(' ', $text);
    }

    private function link(Link $link, string $repository): void
    {
        $url = $link->getUrl();
        if (preg_match('#^https://github\.com/([^/]+/[^/]+)/(pull|issues)/([0-9]{1,9})/?$#i', $url, $match) === 1
            && strtolower($match[1]) === $repository) {
            $label = trim(StringContainerHelper::getChildText($link));
            if ($label === $url || $label === '#'.$match[3] || $label === $match[3]) {
                foreach ($link->children() as $child) {
                    $child->detach();
                }
                $link->appendChild(new Text('#'.$match[3]));
                $link->data->set('attributes/class', 'release-ref');
                $link->data->set('attributes/aria-label', ($match[2] === 'pull' ? 'Pull request ' : 'Issue ').$match[3].' on GitHub');
            }
        }

        // GitHub shows compare links by their range; keep the same short label for bare URLs.
        if (preg_match('~^https://github\.com/([^/]+/[^/]+)/compare/([^/?#]{1,120})$~i', $url, $match) === 1
            && strtolower($match[1]) === $repository
            && trim(StringContainerHelper::getChildText($link)) === $url) {
            foreach ($link->children() as $child) {
                $child->detach();
            }
            $link->appendChild(new Text(rawurldecode($match[2])));
        }

        if (preg_match('#^https?://#i', $url) === 1) {
            $link->data->set('attributes/target', '_blank');
            $link->data->set('attributes/rel', 'noopener noreferrer nofollow');
        }
    }

    private function image(Image $image): void
    {
        $url = $image->getUrl();
        $parts = parse_url($url);
        $origin = is_array($parts) && ($parts['scheme'] ?? null) === 'https' && isset($parts['host'])
            ? 'https://'.strtolower($parts['host']).(isset($parts['port']) ? ':'.$parts['port'] : '')
            : null;
        $path = is_array($parts) ? (string) ($parts['path'] ?? '') : '';
        $allowed = $origin !== null
            && in_array($origin, array_map('strtolower', (array) config('changelog.image_origins')), true)
            && (preg_match('/\.(?:png|jpe?g|gif|webp|avif)$/i', $path) === 1 || str_starts_with($path, '/user-attachments/assets/'));

        if ($allowed) {
            $image->data->set('attributes/loading', 'lazy');
            $image->data->set('attributes/decoding', 'async');
            $image->data->set('attributes/referrerpolicy', 'no-referrer');

            return;
        }

        // Unapproved media is never fetched; keep its description as a labelled source link.
        $alt = trim(StringContainerHelper::getChildText($image));
        $replacement = new Link($url, null, $image->getTitle());
        $replacement->appendChild(new Text($alt === '' ? 'Image' : 'Image: '.$alt));
        $replacement->data->set('attributes/class', 'release-media-link');
        $image->replaceWith($replacement);
        if (preg_match('#^https?://#i', $url) === 1) {
            $replacement->data->set('attributes/target', '_blank');
            $replacement->data->set('attributes/rel', 'noopener noreferrer nofollow');
        }
    }

    /** Moves a leading "**Planned:**" marker to a trailing chip on the list item. */
    private function planned(ListItem $item): void
    {
        $paragraph = $item->firstChild();
        $strong = $paragraph instanceof Paragraph ? $paragraph->firstChild() : null;
        if (! $paragraph instanceof Paragraph || ! $strong instanceof Strong
            || preg_match('/^planned:?$/i', trim(StringContainerHelper::getChildText($strong))) !== 1) {
            return;
        }

        $next = $strong->next();
        if ($next instanceof Text) {
            $next->setLiteral(ltrim(preg_replace('/^:/', '', $next->getLiteral()) ?? ''));
        }
        $strong->detach();
        foreach ($strong->children() as $child) {
            $child->detach();
        }
        $strong->appendChild(new Text('Planned'));
        $strong->data->set('attributes/class', 'release-chip');
        $paragraph->appendChild(new Text(' '));
        $paragraph->appendChild($strong);
    }

    /** Links bare #123 references and @mentions the way GitHub renders release bodies. */
    private function references(Text $text, string $repository): void
    {
        if ($text->parent() === null) {
            return;
        }
        for ($parent = $text->parent(); $parent !== null; $parent = $parent->parent()) {
            if ($parent instanceof Link || $parent instanceof Image) {
                return;
            }
        }

        $literal = $text->getLiteral();
        $pattern = '/(?<![\w&\/#@.-])(?:#([0-9]{1,7})|@([A-Za-z0-9](?:[A-Za-z0-9]|-(?=[A-Za-z0-9])){0,38}))(?![\w-])/';
        if (preg_match_all($pattern, $literal, $matches, PREG_SET_ORDER | PREG_OFFSET_CAPTURE) === 0) {
            return;
        }

        $offset = 0;
        $nodes = [];
        foreach ($matches as $match) {
            [$whole, $position] = $match[0];
            if ($position > $offset) {
                $nodes[] = new Text(substr($literal, $offset, $position - $offset));
            }
            if (($match[1][1] ?? -1) >= 0) {
                $link = new Link('https://github.com/'.config('changelog.repository').'/issues/'.$match[1][0]);
                $link->appendChild(new Text($whole));
                $link->data->set('attributes/class', 'release-ref');
                $link->data->set('attributes/aria-label', 'Issue or pull request '.$match[1][0].' on GitHub');
            } else {
                $link = new Link('https://github.com/'.($match[2][0] ?? ''));
                $link->appendChild(new Text($whole));
                $link->data->set('attributes/class', 'release-mention');
            }
            $link->data->set('attributes/target', '_blank');
            $link->data->set('attributes/rel', 'noopener noreferrer nofollow');
            $nodes[] = $link;
            $offset = $position + strlen($whole);
        }
        if ($offset < strlen($literal)) {
            $nodes[] = new Text(substr($literal, $offset));
        }

        foreach ($nodes as $node) {
            $text->insertBefore($node);
        }
        $text->detach();
    }

    private function normalize(string $text): string
    {
        return mb_substr(trim((string) preg_replace('/\s+/u', ' ', mb_strtolower($text))), 0, 100_000);
    }
}
