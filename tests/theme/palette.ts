import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';

export const profiles: Record<
    string,
    Record<'light' | 'dark', Record<string, string>>
> = JSON.parse(
    execFileSync('php', [resolve('scripts/themes/palette.php'), '--contracts'], {
        encoding: 'utf8',
    }),
).profiles;

export function cssRgb(hex: string): string {
    return `rgb(${[1, 3, 5].map((offset) => Number.parseInt(hex.slice(offset, offset + 2), 16)).join(', ')})`;
}
