import { defineComponent, h, type PropType, type VNodeArrayChildren } from 'vue';
import type { ReleaseNode } from '../../types/changelog';

const globalAttributes = ['id', 'class', 'role', 'aria-label', 'data-kind'];

// Mirrors the server allowlist so a stored document can only ever build these elements.
const elements: Record<string, string[]> = {
    section: [],
    div: [],
    p: [],
    blockquote: [],
    pre: [],
    code: [],
    hr: [],
    br: [],
    h1: [],
    h2: [],
    h3: [],
    h4: [],
    h5: [],
    h6: [],
    ul: [],
    ol: ['start'],
    li: [],
    em: [],
    strong: [],
    del: [],
    sup: [],
    a: ['href', 'title', 'target', 'rel'],
    img: ['src', 'alt', 'title', 'loading', 'decoding', 'referrerpolicy', 'width', 'height'],
    table: [],
    thead: [],
    tbody: [],
    tr: [],
    th: ['align'],
    td: ['align'],
    input: ['type', 'checked', 'disabled'],
};
const voidElements = new Set(['img', 'hr', 'br', 'input']);
const booleanAttributes = new Set(['checked', 'disabled']);

function attributes(
    tag: string,
    source: Record<string, string> = {},
): Record<string, string | boolean> | null {
    const allowed = [...globalAttributes, ...(elements[tag] ?? [])];
    const result: Record<string, string | boolean> = {};
    for (const [name, value] of Object.entries(source)) {
        if (!allowed.includes(name) || typeof value !== 'string') continue;
        result[name] = booleanAttributes.has(name) ? true : value;
    }
    if (
        tag === 'a' &&
        typeof result.href === 'string' &&
        !/^(?:https?:\/\/|mailto:|#)/i.test(result.href)
    )
        return null;
    if (tag === 'img' && !/^https:\/\//i.test(String(result.src ?? ''))) return null;
    if (tag === 'input' && result.type !== 'checkbox') return null;
    if (tag === 'input') result.disabled = true;
    return result;
}

function escapePattern(term: string): string {
    return term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function text(value: string, pattern: RegExp | null): VNodeArrayChildren {
    if (!pattern) return [value];
    // Splitting on a captured pattern alternates plain text (even) and matches (odd).
    return value
        .split(pattern)
        .map((part, index) => (index % 2 ? h('mark', { 'data-search': '' }, part) : part))
        .filter((part) => part !== '');
}

function build(nodes: ReleaseNode[], pattern: RegExp | null): VNodeArrayChildren {
    const children: VNodeArrayChildren = [];
    for (const node of nodes) {
        if (typeof node === 'string') {
            children.push(...text(node, pattern));
            continue;
        }
        if (!node || typeof node !== 'object' || !(node.t in elements)) continue;
        const props = attributes(node.t, node.a);
        const content = Array.isArray(node.c) ? build(node.c, pattern) : [];
        if (!props) {
            if (!voidElements.has(node.t)) children.push(...content);
            continue;
        }
        children.push(voidElements.has(node.t) ? h(node.t, props) : h(node.t, props, content));
    }
    return children;
}

/** Builds a release content block from its node tree, marking search matches as it renders. */
export const ReleaseNodes = defineComponent({
    name: 'ReleaseNodes',
    props: {
        nodes: { type: Array as PropType<ReleaseNode[]>, required: true },
        highlight: { type: String, default: '' },
    },
    setup(props) {
        return () => {
            const term = props.highlight.trim();
            return build(
                props.nodes,
                term.length < 2 ? null : new RegExp(`(${escapePattern(term)})`, 'gi'),
            );
        };
    },
});
