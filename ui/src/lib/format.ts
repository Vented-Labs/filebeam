export function formatBytes(value: number): string {
    if (value < 1024) return `${value} B`;
    const units = ['KiB', 'MiB', 'GiB', 'TiB'];
    const exponent = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length);
    return `${(value / 1024 ** exponent).toFixed(1)} ${units[exponent - 1]}`;
}

export function ciphertextBytes(size: number, chunkBytes: number): number {
    return size + (Math.ceil(size / chunkBytes) || 1) * 16;
}
