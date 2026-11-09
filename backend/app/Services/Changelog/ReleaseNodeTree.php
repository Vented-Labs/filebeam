<?php

declare(strict_types=1);

namespace App\Services\Changelog;

use Dom\Element;
use Dom\HTMLDocument;
use Dom\Node;
use Dom\Text;

/**
 * Converts renderer HTML into a JSON node tree that the client builds element by element, so
 * release bodies never reach the page as markup. Only allowlisted HTML elements and attributes
 * survive; other elements are unwrapped, and foreign or active content is dropped with its text.
 *
 * @phpstan-type ReleaseNode string|array{t: string, a?: array<string, string>, c?: list<mixed>}
 */
final class ReleaseNodeTree
{
    private const string HtmlNamespace = 'http://www.w3.org/1999/xhtml';

    private const array GlobalAttributes = ['id', 'class', 'role', 'aria-label', 'data-kind'];

    /** Elements the client may render, with the attributes each accepts beyond the global set. */
    private const array Elements = [
        'section' => [], 'div' => [], 'p' => [], 'blockquote' => [], 'pre' => [], 'code' => [], 'hr' => [], 'br' => [],
        'h1' => [], 'h2' => [], 'h3' => [], 'h4' => [], 'h5' => [], 'h6' => [],
        'ul' => [], 'ol' => ['start'], 'li' => [],
        'em' => [], 'strong' => [], 'del' => [], 'sup' => [],
        'a' => ['href', 'title', 'target', 'rel'],
        'img' => ['src', 'alt', 'title', 'loading', 'decoding', 'referrerpolicy', 'width', 'height'],
        'table' => [], 'thead' => [], 'tbody' => [], 'tr' => [], 'th' => ['align'], 'td' => ['align'],
        'input' => ['type', 'checked', 'disabled'],
    ];

    private const array VoidElements = ['img', 'hr', 'br', 'input'];

    private const array DroppedElements = ['script', 'style', 'template', 'iframe', 'object', 'embed', 'form', 'button', 'select', 'textarea', 'noscript', 'canvas', 'video', 'audio'];

    /** @return list<ReleaseNode> */
    public static function fromHtml(string $html): array
    {
        $container = HTMLDocument::createEmpty()->createElement('div');
        $container->innerHTML = $html;

        return self::children($container);
    }

    /**
     * Visible text of a node list, for summaries. Accepts stored documents, so entries are untrusted.
     *
     * @param  array<mixed>  $nodes
     */
    public static function text(array $nodes): string
    {
        $text = '';
        foreach ($nodes as $node) {
            $text .= match (true) {
                is_string($node) => $node,
                is_array($node) && is_array($node['c'] ?? null) => self::text($node['c']),
                default => '',
            };
        }

        return $text;
    }

    /** @return list<ReleaseNode> */
    private static function children(Node $parent): array
    {
        $nodes = [];
        foreach ($parent->childNodes as $child) {
            foreach (self::node($child) as $node) {
                $last = array_key_last($nodes);
                if (is_string($node) && $last !== null && is_string($nodes[$last])) {
                    $nodes[$last] .= $node;
                } else {
                    $nodes[] = $node;
                }
            }
        }

        return $nodes;
    }

    /** @return list<ReleaseNode> */
    private static function node(Node $node): array
    {
        if ($node instanceof Text) {
            return $node->data === '' ? [] : [$node->data];
        }
        if (! $node instanceof Element) {
            return [];
        }

        $tag = strtolower($node->localName);
        // SVG and MathML arrive in their own namespaces and never render.
        if ($node->namespaceURI !== self::HtmlNamespace || in_array($tag, self::DroppedElements, true)) {
            return [];
        }
        if (! array_key_exists($tag, self::Elements)) {
            return self::children($node);
        }

        $attributes = self::attributes($node, $tag);
        if ($attributes === null) {
            return in_array($tag, self::VoidElements, true) ? [] : self::children($node);
        }

        $element = ['t' => $tag];
        if ($attributes !== []) {
            $element['a'] = $attributes;
        }
        $children = in_array($tag, self::VoidElements, true) ? [] : self::children($node);
        if ($children !== []) {
            $element['c'] = $children;
        }

        return [$element];
    }

    /** @return array<string, string>|null Null when the element cannot be kept safely. */
    private static function attributes(Element $element, string $tag): ?array
    {
        $allowed = [...self::GlobalAttributes, ...self::Elements[$tag]];
        $attributes = [];
        foreach ($element->attributes as $attribute) {
            $name = strtolower($attribute->name);
            if (in_array($name, $allowed, true)) {
                $attributes[$name] = $attribute->value;
            }
        }

        return match ($tag) {
            'a' => isset($attributes['href']) && preg_match('/^(?:https?:\/\/|mailto:|#)/i', $attributes['href']) !== 1 ? null : $attributes,
            'img' => str_starts_with(strtolower($attributes['src'] ?? ''), 'https://') ? $attributes : null,
            'input' => ($attributes['type'] ?? '') === 'checkbox' ? [...$attributes, 'disabled' => ''] : null,
            default => $attributes,
        };
    }
}
