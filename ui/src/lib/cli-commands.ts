import { decodeBase64Url, encodeBase64Url } from './base64url';
import type { CliConfig } from '../types';
export { detectDesktopPlatform } from './platform';
export type { DesktopPlatform as CliPlatform, UserAgentDetails } from './platform';
import type { DesktopPlatform as CliPlatform } from './platform';

export const cliDefaults: CliConfig = {
    installer_url: 'https://releases.filebeam.io/cli/install.sh',
    windows_installer_url: 'https://releases.filebeam.io/cli/install.ps1',
    installer_interpreter: 'sh',
    executable: 'beam',
};

export function quoteShellArgument(value: string): string {
    if (!value || /\p{Cc}/u.test(value))
        throw new Error('A nonempty argument without control characters is required.');
    return `'${value.replaceAll("'", "'\"'\"'")}'`;
}

export function quotePowerShellArgument(value: string): string {
    if (!value || /\p{Cc}/u.test(value))
        throw new Error('A nonempty argument without control characters is required.');
    return `'${value.replaceAll("'", "''")}'`;
}

function webUrl(value: string): URL {
    quoteShellArgument(value);
    if (!/^https?:\/\//i.test(value)) throw new Error('A full HTTP transfer URL is required.');
    const url = new URL(value);
    if (!url.hostname || url.username || url.password)
        throw new Error('URL credentials are not supported.');
    return url;
}

function publishedInstallerUrl(value: string | null | undefined): string | undefined {
    if (!value) return;
    const url = webUrl(value);
    if (url.protocol !== 'https:' || url.hash || /(?:^|\.)example$/i.test(url.hostname))
        throw new Error('A published HTTPS installer is required.');
    return value;
}

export function buildInstallCommand(
    config: CliConfig,
    platform: CliPlatform = 'linux',
): string | undefined {
    if (platform === 'windows') {
        const url = publishedInstallerUrl(config.windows_installer_url);
        return url && `Invoke-RestMethod -Uri ${quotePowerShellArgument(url)} | Invoke-Expression`;
    }
    if (config.installer_interpreter !== 'sh')
        throw new Error('A published HTTPS shell installer is required.');
    const url = publishedInstallerUrl(config.installer_url);
    return url && `curl -fsSL ${quoteShellArgument(url)} | sh`;
}

export function buildDownloadCommand(target: string): string {
    quoteShellArgument(target);
    const ulid = /^[0-7][0-9A-HJKMNP-TV-Z]{25}$/i;
    if (ulid.test(target)) return `beam down ${quoteShellArgument(target)}`;
    const url = webUrl(target);
    // Match crates/cli/src/protocol.rs: one ULID path, no query, and an unencoded v1 key.
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
    pending?: boolean;
    turbo?: boolean;
    expiresAt?: string;
    inbox?: boolean;
    burnOnRead?: boolean;
};

export function cliUnavailableReason(transfer: CliTransfer, now = Date.now()): string | undefined {
    if (!transfer.available) return 'This transfer is not currently available for CLI download.';
    if (transfer.expiresAt && !(Date.parse(transfer.expiresAt) > now))
        return 'This transfer has expired.';
    if (transfer.pending && !transfer.turbo)
        return 'This transfer is still being prepared for CLI download.';
    if (transfer.inbox) return 'Use the browser to unlock files received with your account key.';
    if (transfer.kind !== 'files' || transfer.burnOnRead)
        return 'Use the browser for notes and burn-on-read transfers.';
}
