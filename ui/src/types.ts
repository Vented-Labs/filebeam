export type TransferDriver = 'http' | 'webrtc';

export type TransferLimits = {
    maximum_transfer_bytes: number | null;
    maximum_file_count: number | null;
    maximum_note_bytes: number | null;
};

export type SocialPlatform =
    | 'discord'
    | 'x'
    | 'bluesky'
    | 'mastodon'
    | 'threads'
    | 'github'
    | 'youtube'
    | 'instagram'
    | 'facebook'
    | 'linkedin'
    | 'reddit'
    | 'telegram'
    | 'tiktok'
    | 'twitch'
    | 'website';

export type CommunityLink = {
    platform: SocialPlatform;
    label: string;
    url: string;
};

export type FilebeamConfig = {
    main_site_url: string;
    cli?: CliConfig;
    github_url: string;
    copyright_holder: string;
    copyright_url?: string | null;
    community_links?: CommunityLink[];
    maximum_transfer_bytes: number;
    maximum_file_count: number;
    maximum_note_bytes: number;
    file_retention_hours: number;
    note_retention_hours: number;
    chunk_bytes: number;
    upload_concurrency?: number;
    download_concurrency?: number;
    upload_transport?: {
        version: 1;
        part_min_bytes: number;
        part_max_bytes: number;
        request_target_ms: number;
        request_budget_ms: number;
    };
    registration_enabled?: boolean;
    anonymous_uploads_enabled: boolean;
    file_retention_options?: number[];
    note_retention_options?: number[];
    transport_policy?: {
        enabled_drivers: TransferDriver[];
        default_driver: TransferDriver;
        limits: Record<TransferDriver, TransferLimits>;
    };
};

export type CliConfig = {
    installer_url: string | null;
    windows_installer_url?: string | null;
    installer_interpreter: 'sh';
    executable: 'beam';
};

export type PublicRecipient = {
    id: number;
    username: string;
    public_key: string;
    account_key_bundle_id: number;
    version: number;
    fingerprint: string;
};
