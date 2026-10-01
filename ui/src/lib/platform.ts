export type DesktopPlatform = 'linux' | 'macos' | 'windows';

export type UserAgentDetails = {
    userAgentDataPlatform?: string;
    userAgentDataMobile?: boolean;
    platform?: string;
    userAgent?: string;
    maxTouchPoints?: number;
};

export function browserPlatformDetails(): UserAgentDetails {
    const browser = navigator as Navigator & {
        userAgentData?: { platform?: string; mobile?: boolean };
    };
    return {
        userAgentDataPlatform: browser.userAgentData?.platform,
        userAgentDataMobile: browser.userAgentData?.mobile,
        platform: browser.platform,
        userAgent: browser.userAgent,
        maxTouchPoints: browser.maxTouchPoints,
    };
}

export function isMobileDevice(details: UserAgentDetails): boolean {
    return Boolean(
        details.userAgentDataMobile ||
        /\b(?:iPad|iPhone|iPod|Android|Mobile|Windows Phone)\b/i.test(details.userAgent ?? '') ||
        ((details.platform ?? '').includes('MacIntel') && (details.maxTouchPoints ?? 0) > 0),
    );
}

export function detectDesktopPlatform(details: UserAgentDetails): DesktopPlatform | undefined {
    if (isMobileDevice(details)) return;
    for (const value of [details.userAgentDataPlatform, details.platform, details.userAgent]) {
        if (!value) continue;
        if (/win/i.test(value)) return 'windows';
        if (/linux/i.test(value)) return 'linux';
        if (/mac/i.test(value)) return 'macos';
    }
}
