import type { DesktopArchitecture, DesktopReleaseResult } from '../types';

export async function loadDesktopRelease(signal: AbortSignal): Promise<DesktopReleaseResult> {
    const response = await fetch('/api/v1/desktop/releases', {
        headers: { Accept: 'application/json' },
        signal,
    });
    if (!response.ok) throw new Error('Unable to load desktop downloads.');
    const result: DesktopReleaseResult = await response.json();
    if (
        result.state !== 'unavailable' &&
        !(result.state === 'available' && result.release?.assets?.length)
    ) {
        throw new Error('Invalid desktop release response.');
    }
    return result;
}

export async function detectDesktopArchitecture(): Promise<DesktopArchitecture | undefined> {
    const browser = navigator as Navigator & {
        userAgentData?: {
            getHighEntropyValues?: (
                hints: string[],
            ) => Promise<{ architecture?: string; bitness?: string }>;
        };
    };
    try {
        const hints = await browser.userAgentData?.getHighEntropyValues?.([
            'architecture',
            'bitness',
        ]);
        if (hints?.bitness !== '64') return;
        if (hints.architecture === 'arm') return 'aarch64';
        if (hints.architecture === 'x86') return 'x86_64';
    } catch {
        // Browsers may withhold architecture; the selector remains available.
    }
}
