export function encodeBase64Url(bytes: Uint8Array): string {
    let binary = '';
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return btoa(binary).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '');
}

export function decodeBase64Url(value: string): Uint8Array {
    if (!/^[A-Za-z0-9_-]+$/.test(value)) throw new Error('Invalid base64url data.');
    return Uint8Array.from(
        atob(
            value
                .replaceAll('-', '+')
                .replaceAll('_', '/')
                .padEnd(Math.ceil(value.length / 4) * 4, '='),
        ),
        (character) => character.charCodeAt(0),
    );
}
