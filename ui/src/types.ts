export type TransferDriver = 'http' | 'webrtc';

export type TransferLimits = {
    maximum_transfer_bytes: number | null;
    maximum_file_count: number | null;
    maximum_note_bytes: number | null;
};

export type FilebeamConfig = {
    github_url: string;
    copyright_holder: string;
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

export type PublicRecipient = {
    id: number;
    username: string;
    public_key: string;
    account_key_bundle_id: number;
    version: number;
    fingerprint: string;
};
