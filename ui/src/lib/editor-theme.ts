import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';

export const filebeamHighlightStyle = HighlightStyle.define([
    { tag: [tags.name, tags.variableName], class: 'fb-syn-variable' },
    { tag: tags.keyword, class: 'fb-syn-keyword' },
    { tag: [tags.string, tags.character, tags.attributeValue], class: 'fb-syn-string' },
    { tag: [tags.number, tags.unit], class: 'fb-syn-number' },
    {
        tag: [tags.bool, tags.null, tags.atom, tags.constant(tags.variableName)],
        class: 'fb-syn-constant',
    },
    { tag: tags.propertyName, class: 'fb-syn-property' },
    { tag: [tags.typeName, tags.className, tags.namespace], class: 'fb-syn-type' },
    {
        tag: [tags.function(tags.variableName), tags.function(tags.propertyName)],
        class: 'fb-syn-function',
    },
    { tag: tags.tagName, class: 'fb-syn-tag' },
    { tag: tags.attributeName, class: 'fb-syn-attribute' },
    { tag: tags.comment, class: 'fb-syn-comment' },
    { tag: tags.punctuation, class: 'fb-syn-punctuation' },
    { tag: tags.operator, class: 'fb-syn-operator' },
    { tag: [tags.meta, tags.annotation, tags.processingInstruction], class: 'fb-syn-meta' },
    { tag: tags.regexp, class: 'fb-syn-regexp' },
    { tag: [tags.escape, tags.special(tags.string)], class: 'fb-syn-escape' },
    { tag: [tags.link, tags.url], class: 'fb-syn-link' },
    { tag: tags.heading, class: 'fb-syn-heading' },
    { tag: tags.emphasis, class: 'fb-syn-emphasis' },
    { tag: tags.strong, class: 'fb-syn-strong' },
    { tag: tags.strikethrough, class: 'fb-syn-strike' },
    { tag: tags.monospace, class: 'fb-syn-monospace' },
    { tag: tags.inserted, class: 'fb-syn-inserted' },
    { tag: tags.deleted, class: 'fb-syn-deleted' },
    { tag: tags.invalid, class: 'fb-syn-invalid' },
]);

export const filebeamHighlighting = syntaxHighlighting(filebeamHighlightStyle);

// Mode is provided only by the editor's appearance compartment. This theme is mode-neutral.
export const filebeamEditorChrome = EditorView.theme({
    '&': {
        height: '100%',
        backgroundColor: 'var(--fb-editor-bg)',
        color: 'var(--fb-editor-fg)',
        fontFamily: 'var(--fb-font-code)',
        fontSize: 'var(--fb-font-code-size)',
        fontVariantLigatures: 'none',
        fontFeatureSettings: '"liga" 0, "calt" 0',
    },
    '&.cm-focused': { outline: 'none' },
    '.cm-scroller': {
        fontFamily: 'var(--fb-font-code)',
        fontSize: 'var(--fb-font-code-size)',
        lineHeight: 'var(--fb-line-code)',
        fontVariantLigatures: 'none',
        fontFeatureSettings: '"liga" 0, "calt" 0',
    },
    '.cm-gutters': {
        backgroundColor: 'var(--fb-editor-gutter-bg)',
        color: 'var(--fb-editor-gutter-fg)',
        borderRight: '1px solid var(--fb-editor-border)',
    },
    // The active line must let CodeMirror's selection layer show through.
    '.cm-activeLine': { backgroundColor: 'var(--fb-editor-active-line)' },
    '.cm-activeLineGutter': {
        backgroundColor: 'var(--fb-editor-gutter-active-bg)',
        color: 'var(--fb-editor-gutter-active-fg)',
    },
    '.cm-selectionBackground': { backgroundColor: 'var(--fb-editor-selection-inactive)' },
    '&.cm-focused .cm-selectionBackground': { backgroundColor: 'var(--fb-editor-selection)' },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--fb-editor-caret)' },
    '.cm-content': { caretColor: 'var(--fb-editor-caret)', padding: '16px 0' },
    '.cm-matchingBracket': {
        backgroundColor: 'var(--fb-editor-matching-bracket)',
        outline: '1px solid var(--fb-editor-bracket-border)',
    },
    '.cm-nonmatchingBracket': {
        color: 'var(--fb-editor-invalid)',
        textDecoration: 'underline wavy',
    },
    '.cm-searchMatch': { backgroundColor: 'var(--fb-editor-search-match)' },
    '.cm-searchMatch-selected': { outline: '1px solid var(--fb-editor-bracket-border)' },
    '.cm-selectionMatch': {
        backgroundColor: 'transparent',
        outline: '1px solid var(--fb-editor-bracket-border)',
    },
    '.cm-specialChar': { color: 'var(--fb-editor-invalid)' },
    '.cm-foldPlaceholder': {
        color: 'var(--fb-editor-comment)',
        backgroundColor: 'var(--fb-editor-gutter-bg)',
        border: '1px solid var(--fb-editor-border)',
    },
    '.cm-tooltip, .cm-panels': {
        backgroundColor: 'var(--fb-surface-raised)',
        color: 'var(--fb-text)',
        border: '1px solid var(--fb-border)',
    },
    '.cm-tooltip': { boxShadow: 'var(--fb-shadow-popover)' },
    '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
        backgroundColor: 'var(--fb-selected-surface)',
        color: 'var(--fb-accent-text)',
    },
    '.cm-textfield': {
        backgroundColor: 'var(--fb-surface-sunken)',
        color: 'var(--fb-text)',
        border: '1px solid var(--fb-control-border)',
    },
    '.cm-button': {
        background: 'var(--fb-secondary-surface)',
        backgroundImage: 'none',
        color: 'var(--fb-secondary-text)',
        border: '1px solid var(--fb-secondary-border)',
    },
    '.cm-button:enabled:hover': {
        backgroundColor: 'var(--fb-secondary-hover-surface)',
        borderColor: 'var(--fb-secondary-hover-border)',
    },
    '.cm-textfield:focus-visible, .cm-button:focus-visible': {
        outline: '2px solid var(--fb-focus)',
        outlineOffset: '3px',
    },
    ...Object.fromEntries(
        [
            'keyword',
            'string',
            'number',
            'constant',
            'property',
            'type',
            'function',
            'tag',
            'attribute',
            'punctuation',
            'operator',
            'meta',
            'regexp',
            'escape',
        ].map((name) => [`.fb-syn-${name}`, { color: `var(--fb-editor-${name})` }]),
    ),
    '.fb-syn-variable': { color: 'var(--fb-editor-fg)' },
    '.fb-syn-comment': { color: 'var(--fb-editor-comment)', fontStyle: 'italic' },
    '.fb-syn-link': {
        color: 'var(--fb-editor-link)',
        textDecoration: 'underline',
        textUnderlineOffset: '0.15em',
    },
    '.fb-syn-heading': { color: 'var(--fb-editor-heading)', fontWeight: '700' },
    '.fb-syn-emphasis': { fontStyle: 'italic' },
    '.fb-syn-strong': { fontWeight: '700' },
    '.fb-syn-strike': { textDecoration: 'line-through' },
    '.fb-syn-monospace': { fontFamily: 'var(--fb-font-code)' },
    '.fb-syn-inserted': { color: 'var(--fb-editor-string)', textDecoration: 'underline' },
    '.fb-syn-deleted': { color: 'var(--fb-editor-invalid)', textDecoration: 'line-through' },
    '.fb-syn-invalid': { color: 'var(--fb-editor-invalid)', textDecoration: 'underline wavy' },
});
