import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { undoDepth, redoDepth, undo, redo } from '@codemirror/commands';
import { syntaxTree } from '@codemirror/language';
import { SearchQuery, setSearchQuery, openSearchPanel } from '@codemirror/search';
import { tags } from '@lezer/highlight';

// This module is served only by the development gallery, never by the application.
function editor(): EditorView {
    const element = document.querySelector('.cm-editor');
    const view = element && EditorView.findFromDOM(element as HTMLElement);
    if (!view) throw new Error('The fixture editor is not mounted.');
    return view;
}

export function inspect() {
    const view = editor();
    return {
        dark: view.state.facet(EditorView.darkTheme),
        document: view.state.doc.toString(),
        selection: view.state.selection.toJSON(),
        undo: undoDepth(view.state),
        redo: redoDepth(view.state),
        scroll: view.scrollDOM.scrollTop,
        wrap: view.lineWrapping,
        readOnly: view.state.facet(EditorState.readOnly),
        tree: syntaxTree(view.state).toString(),
    };
}

export function select(from: number, to: number) {
    const view = editor();
    view.dispatch({ selection: { anchor: from, head: to } });
    view.focus();
}

export function history(direction: 'undo' | 'redo') {
    return (direction === 'undo' ? undo : redo)(editor());
}

export function scroll(top: number) {
    editor().scrollDOM.scrollTop = top;
}

export function search(text: string) {
    openSearchPanel(editor());
    editor().dispatch({ effects: setSearchQuery.of(new SearchQuery({ search: text })) });
}

export async function tagClasses() {
    const { filebeamHighlightStyle } = await import('../../ui/src/lib/editor-theme');
    return Object.fromEntries(
        [
            ['variable', tags.variableName],
            ['keyword', tags.keyword],
            ['string', tags.string],
            ['number', tags.number],
            ['constant', tags.constant(tags.variableName)],
            ['property', tags.propertyName],
            ['type', tags.typeName],
            ['function', tags.function(tags.variableName)],
            ['tag', tags.tagName],
            ['attribute', tags.attributeName],
            ['comment', tags.comment],
            ['punctuation', tags.punctuation],
            ['operator', tags.operator],
            ['meta', tags.meta],
            ['regexp', tags.regexp],
            ['escape', tags.escape],
            ['link', tags.link],
            ['heading', tags.heading],
            ['emphasis', tags.emphasis],
            ['strong', tags.strong],
            ['strike', tags.strikethrough],
            ['monospace', tags.monospace],
            ['inserted', tags.inserted],
            ['deleted', tags.deleted],
            ['invalid', tags.invalid],
        ].map(([name, tag]) => [name, filebeamHighlightStyle.style([tag as typeof tags.keyword])]),
    );
}
