import type { TransferDriver } from './types';

export type TransferMode = 'files' | 'note';

export type UploadFileState = 'queued' | 'encrypting' | 'uploading' | 'complete' | 'error';

export type UploadEntry = {
    id: string;
    file: File;
    name: string;
    type: string;
    state: UploadFileState;
    progress: number;
    error?: string;
};

export type ShareResult = {
    link: string;
    key: string;
    deleteToken: string;
    expiresAt?: string;
    transferId: string;
    includeKey: boolean;
    passwordProtected: boolean;
    turbo?: boolean;
    monitorToken?: string;
    driver?: TransferDriver;
};

export type DownloadSession = {
    id: string;
    number: number;
    progress: number;
    status: 'downloading' | 'waiting' | 'verifying' | 'completed' | 'cancelled' | 'error' | 'stale';
    selection_count: number;
    all_files: boolean;
};
