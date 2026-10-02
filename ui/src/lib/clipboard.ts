export type PasteContent = { files: File[]; text: string };

function namedFile(blob: Blob, name?: string): File {
    const extension = blob.type.split('/')[1]?.split('+')[0] || 'bin';
    return new File([blob], name || `pasted-image-${crypto.randomUUID()}.${extension}`, {
        type: blob.type,
    });
}

export function pastedContent(data: DataTransfer): PasteContent {
    const originals = data.files.length
        ? Array.from(data.files)
        : Array.from(data.items)
              .filter((item) => item.kind === 'file')
              .map((item) => item.getAsFile())
              .filter((file): file is File => file !== null);
    const files = originals.map((file) => namedFile(file, file.name));
    return { files, text: files.length ? '' : data.getData('text/plain') };
}

export async function readClipboard(): Promise<PasteContent> {
    if (navigator.clipboard?.read) {
        const items = await navigator.clipboard.read();
        const files: File[] = [];
        for (const item of items) {
            const type = item.types.find((type) => type.startsWith('image/'));
            if (type) files.push(namedFile(await item.getType(type)));
        }
        if (files.length) return { files, text: '' };
        const texts = await Promise.all(
            items
                .filter((item) => item.types.includes('text/plain'))
                .map(async (item) => (await item.getType('text/plain')).text()),
        );
        return { files: [], text: texts.join('\n') };
    }
    if (navigator.clipboard?.readText)
        return { files: [], text: await navigator.clipboard.readText() };
    throw new Error('Clipboard reading is unavailable.');
}
