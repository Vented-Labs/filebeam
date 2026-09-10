import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import {
    buildDownloadCommand,
    buildInstallCommand,
    cliDefaults,
    cliUnavailableReason,
    detectDesktopPlatform,
    quotePowerShellArgument,
    quoteShellArgument,
} from '../../ui/src/lib/cli-commands';
import { buildShareLink } from '../../ui/src/lib/share-link';

const id = '01K46FN13WJVCWKMBWRMC9Q9KN';
const fragment = `#k=v1.${'A'.repeat(43)}`;

test('CLI shell arguments round-trip literally and reject controls', () => {
    for (const value of [
        "apostrophe's",
        '$(printf BAD);`printf BAD` & ! * ?',
        'space and 雪',
        'a%20b',
        'a'.repeat(6000),
    ]) {
        expect(
            execFileSync('/bin/sh', ['-c', `printf '%s' ${quoteShellArgument(value)}`], {
                encoding: 'utf8',
            }),
        ).toBe(value);
    }
    for (const value of ['', 'a\nb', 'a\0b', 'a\rb', 'a\x7fb'])
        expect(() => quoteShellArgument(value)).toThrow();
});

test('CLI targets follow the parser and preserve canonical keyed and keyless links', () => {
    const keyed = buildShareLink(`/${id}`, 'https://filebeam.io', fragment);
    expect(buildDownloadCommand(keyed)).toBe(`beam down '${keyed}'`);
    expect(buildDownloadCommand(buildShareLink(`/${id}`, 'https://filebeam.io'))).toBe(
        `beam down 'https://filebeam.io/${id}'`,
    );
    expect(buildDownloadCommand(id)).toBe(`beam down '${id}'`);
    const hosted = `https://files.company.test:8443/${id}${fragment}`;
    expect(buildDownloadCommand(hosted)).toBe(`beam down '${hosted}'`);
    const local =
        'http://localhost:8000/01M23GEFKC2WNXBASCS675XJRD#k=v1.KGGOo1frIIN3t1kAgQ2STIKjGXOFPKiPBATHBKDpzxY';
    expect(buildDownloadCommand(local)).toBe(`beam down '${local}'`);
    for (const target of [
        '--help',
        `/${id}`,
        `javascript:${id}`,
        `https://u:p@filebeam.io/${id}`,
        `https://filebeam.io/f/${id}`,
        `https://filebeam.io/${id}?a=1`,
        `https://filebeam.io/${id}#k=invalid`,
        `https://filebeam.io/${id}${fragment}&extra=1`,
        `https://filebeam.io/${id}\n`,
    ])
        expect(() => buildDownloadCommand(target)).toThrow();
});

test('platform installers stream directly to the selected shell', () => {
    expect(buildInstallCommand({ ...cliDefaults, installer_url: null })).toBeUndefined();
    expect(buildInstallCommand(cliDefaults)).toBe(
        "curl -fsSL 'https://releases.filebeam.io/cli/install.sh' | sh",
    );
    const url = "https://releases.filebeam.test/cli/install.sh?tag=it's-ready&v=1";
    expect(buildInstallCommand({ ...cliDefaults, installer_url: url })).toBe(
        `curl -fsSL ${quoteShellArgument(url)} | sh`,
    );
    const windowsUrl = "https://releases.filebeam.test/cli/install.ps1?tag=it's-ready";
    expect(
        buildInstallCommand({ ...cliDefaults, windows_installer_url: windowsUrl }, 'windows'),
    ).toBe(`Invoke-RestMethod -Uri ${quotePowerShellArgument(windowsUrl)} | Invoke-Expression`);
    expect(buildInstallCommand(cliDefaults, 'windows')).toBe(
        "Invoke-RestMethod -Uri 'https://releases.filebeam.io/cli/install.ps1' | Invoke-Expression",
    );
    for (const installer_url of [
        'http://filebeam.io/install.sh',
        'https://filebeam.example/install.sh',
        'https://u:p@filebeam.io/install.sh',
        'https://filebeam.io/install.sh#run',
    ])
        expect(() => buildInstallCommand({ ...cliDefaults, installer_url })).toThrow();
    expect(() =>
        buildInstallCommand(
            { ...cliDefaults, windows_installer_url: 'http://filebeam.io/install.ps1' },
            'windows',
        ),
    ).toThrow();
});

test('desktop platform detection ignores mobile and iPadOS user agents', () => {
    expect(detectDesktopPlatform({ userAgentDataPlatform: 'Linux', platform: 'MacIntel' })).toBe(
        'linux',
    );
    expect(
        detectDesktopPlatform({ userAgentDataPlatform: 'Windows', userAgentDataMobile: true }),
    ).toBeUndefined();
    expect(
        detectDesktopPlatform({
            platform: 'MacIntel',
            userAgent: 'Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X)',
        }),
    ).toBeUndefined();
    expect(
        detectDesktopPlatform({
            platform: 'MacIntel',
            userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15)',
            maxTouchPoints: 5,
        }),
    ).toBeUndefined();
    expect(detectDesktopPlatform({ platform: 'Linux x86_64' })).toBe('linux');
    expect(detectDesktopPlatform({ userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)' })).toBe(
        'windows',
    );
    expect(
        detectDesktopPlatform({ userAgent: 'Mozilla/5.0 (X11; Linux x86_64) Mobile' }),
    ).toBeUndefined();
    expect(detectDesktopPlatform({ platform: 'Unknown', userAgent: 'Unknown' })).toBeUndefined();
});

test('installer publication and real CLI capability gate actionable commands', () => {
    const ready = { kind: 'files' as const, driver: 'http' as const, available: true };
    expect(cliUnavailableReason(ready)).toBeUndefined();
    for (const transfer of [
        { ...ready, available: false },
        { ...ready, kind: 'note' as const },
        { ...ready, driver: 'webrtc' as const },
        { ...ready, inbox: true },
        { ...ready, burnOnRead: true },
        { ...ready, expiresAt: '2000-01-01T00:00:00Z' },
        { ...ready, expiresAt: 'invalid' },
    ])
        expect(cliUnavailableReason(transfer)).toBeTruthy();
});
