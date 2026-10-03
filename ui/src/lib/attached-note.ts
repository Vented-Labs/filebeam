import { noteLanguages } from './note-languages';

export const MAX_ATTACHED_NOTE_BYTES = 64 * 1024;
export type AttachedNote = { text: string; title?: string; language: string };

export function validateAttachedNote(value: unknown): AttachedNote {
    if (!value || typeof value !== 'object') throw new Error('Invalid attached note.');
    const note = value as Partial<AttachedNote>;
    if (
        typeof note.text !== 'string' ||
        !note.text.length ||
        new TextEncoder().encode(note.text).length > MAX_ATTACHED_NOTE_BYTES
    )
        throw new Error('Attached note text must contain 1 byte to 64 KiB of UTF-8.');
    if (
        note.title !== undefined &&
        (typeof note.title !== 'string' || Array.from(note.title).length > 160)
    )
        throw new Error('Attached note title must contain at most 160 characters.');
    if (!noteLanguages.some((language) => language.value === note.language))
        throw new Error('Unsupported attached note language.');
    return {
        text: note.text,
        ...(note.title !== undefined ? { title: note.title } : {}),
        language: note.language!,
    };
}

export function sameAttachedNote(a?: AttachedNote, b?: AttachedNote): boolean {
    return a?.text === b?.text && a?.title === b?.title && a?.language === b?.language;
}

export function checkAttachedMetadataSize(
    note: AttachedNote,
    files: Array<{ name: string; type: string; size: number }>,
    chunkBytes: number,
): void {
    const manifest = {
        version: 1,
        attached_note: note,
        items: files.map((file) => ({
            id: '0'.repeat(26),
            name: file.name,
            type: file.type,
            size: file.size,
            nonce_prefix: '0'.repeat(22),
            chunk_count: Math.max(1, Math.ceil(file.size / chunkBytes)),
            digest: { algorithm: 'sha256', value: '0'.repeat(64) },
        })),
    };
    if (
        Math.ceil(((new TextEncoder().encode(JSON.stringify(manifest)).length + 16) * 4) / 3) +
            512 >
        524288
    )
        throw new Error(
            'Encrypted transfer metadata exceeds 512 KiB; shorten the note or filenames.',
        );
}

export function saveAttachedNote(note: AttachedNote): void {
    const url = URL.createObjectURL(new Blob([note.text], { type: 'text/plain;charset=utf-8' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = 'attached-note.txt';
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
}
