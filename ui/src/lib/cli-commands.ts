import { decodeBase64Url, encodeBase64Url } from './base64url';
import type { CliConfig } from '../types';

export const cliDefaults: CliConfig = {
    installer_url: 'https://releases.filebeam.io/cli/install.sh',
    installer_interpreter: 'sh',
    executable: 'beam',
};

export function quoteShellArgument(value: string): string {
    if (!value || /\p{Cc}/u.test(value))
        throw new Error('A nonempty argument without control characters is required.');
    return `'${value.replaceAll("'", "'\"'\"'")}'`;
}

function webUrl(value: string): URL {
    quoteShellArgument(value);
    if (!/^https?:\/\//i.test(value)) throw new Error('A full HTTP transfer URL is required.');
    const url = new URL(value);
    if (!url.hostname || url.username || url.password)
        throw new Error('URL credentials are not supported.');
    return url;
}

export function buildInstallCommand(config: CliConfig): string | undefined {
    if (!config.installer_url) return;
    const url = webUrl(config.installer_url);
    if (
        url.protocol !== 'https:' ||
        url.hash ||
        /(?:^|\.)example$/i.test(url.hostname) ||
        config.installer_interpreter !== 'sh'
    )
        throw new Error('A published HTTPS shell installer is required.');
    return `curl -fsSL ${quoteShellArgument(config.installer_url)} -o beam-install.sh && sh beam-install.sh`;
}

export function buildDownloadCommand(target: string): string {
    quoteShellArgument(target);
    const ulid = /^[0-7][0-9A-HJKMNP-TV-Z]{25}$/i;
    if (ulid.test(target)) return `beam down ${quoteShellArgument(target)}`;
    const url = webUrl(target);
    // Match cli/src/protocol.rs: one ULID path, no query, and an unencoded v1 key.
    if (url.search || !ulid.test(url.pathname.slice(1)))
        throw new Error('This link format is not supported by the CLI.');
    if (url.hash) {
        const key = url.hash.match(/^#(?:k=)?v1\.([A-Za-z0-9_-]{43})$/)?.[1];
        if (!key || encodeBase64Url(decodeBase64Url(key)) !== key)
            throw new Error('This key fragment is not supported by the CLI.');
    }
    // Preserve the raw target, including the user's permitted fragment, byte for byte.
    return `beam down ${quoteShellArgument(target)}`;
}

export type CliTransfer = {
    kind: 'files' | 'note';
    driver: 'http' | 'webrtc';
    available: boolean;
    expiresAt?: string;
    inbox?: boolean;
    burnOnRead?: boolean;
};

export function cliUnavailableReason(transfer: CliTransfer, now = Date.now()): string | undefined {
    if (!transfer.available) return 'This transfer is not currently available for CLI download.';
    if (transfer.expiresAt && !(Date.parse(transfer.expiresAt) > now))
        return 'This transfer has expired.';
    if (transfer.driver === 'webrtc')
        return 'The CLI supports stored HTTP files. Use the browser for this live transfer and keep the sender’s tab open.';
    if (transfer.inbox) return 'Use the browser to unlock files received with your account key.';
    if (transfer.kind !== 'files' || transfer.burnOnRead)
        return 'Use the browser for notes and burn-on-read transfers.';
}
