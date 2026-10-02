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
