export type NoteLanguage =
    | 'plain'
    | 'php'
    | 'dotenv'
    | 'javascript'
    | 'typescript'
    | 'json'
    | 'markdown'
    | 'css'
    | 'html';

export type NoteLanguageOption = {
    value: NoteLanguage;
    label: string;
    extension: string;
};

export const noteLanguages: readonly NoteLanguageOption[] = [
    { value: 'plain', label: 'Plain text', extension: 'txt' },
    { value: 'php', label: 'PHP', extension: 'php' },
    { value: 'dotenv', label: '.ENV', extension: 'env' },
    { value: 'javascript', label: 'JavaScript', extension: 'js' },
    { value: 'typescript', label: 'TypeScript', extension: 'ts' },
    { value: 'json', label: 'JSON', extension: 'json' },
    { value: 'markdown', label: 'Markdown', extension: 'md' },
    { value: 'css', label: 'CSS', extension: 'css' },
    { value: 'html', label: 'HTML', extension: 'html' },
] as const;

export function normalizeNoteLanguage(value: string): NoteLanguage {
    return noteLanguages.some((language) => language.value === value)
        ? (value as NoteLanguage)
        : 'plain';
}

export function noteFilename(language: string): string {
    const selected = noteLanguages.find((item) => item.value === normalizeNoteLanguage(language));
    return `untitled.${selected?.extension ?? 'txt'}`;
}
