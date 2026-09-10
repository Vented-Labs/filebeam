import type { UploadTransport } from './adaptive-upload';
import { transferPolicy } from './transfer';

export type StageSnapshot = {
    id: string;
    state: 'receiving' | 'finalizing' | 'complete';
    offset: number;
    ciphertext_bytes: number;
    checksum: string;
};

export class StageSession {
    private readonly inner: InstanceType<
        ReturnType<typeof transferPolicy>['StageSessionController']
    >;

    constructor(id: string, ciphertextBytes: number, checksum: string) {
        this.inner = new (transferPolicy().StageSessionController)(id, ciphertextBytes, checksum);
    }

    reconcile(status: unknown): StageSnapshot {
        return this.inner.reconcile(stageStatus(status));
    }

    acknowledgePart(start: number, bytes: number, status: unknown): StageSnapshot {
        return this.inner.acknowledgePart(start, bytes, stageStatus(status));
    }

    recordRetry(): void {
        this.inner.recordRetry();
    }

    reset(id: string): void {
        this.inner.reset(id);
    }

    reprobeAfterRecovery(): boolean {
        return this.inner.reprobeAfterRecovery();
    }

    get retries(): number {
        return this.inner.retries;
    }
}

function stageStatus(value: unknown): unknown {
    let status = value;
    while (status && typeof status === 'object' && 'data' in status)
        status = (status as { data?: unknown }).data;
    return status;
}

export function validateUploadTransport(transport: UploadTransport): void {
    transferPolicy().validateUploadTransport(transport);
}

export function partBytes(transport: UploadTransport, rate: number): number {
    return transferPolicy().partBytes(transport, rate);
}

export function initialPartBytes(transport: UploadTransport, rate: number): number {
    return transferPolicy().initialPartBytes(transport, rate);
}

export function growPart(
    transport: UploadTransport,
    previous: number,
    bytes: number,
    elapsedMs: number,
): number {
    return transferPolicy().growPart(transport, previous, bytes, Math.max(1, Math.ceil(elapsedMs)));
}

export function shrinkPart(transport: UploadTransport, previous: number): number {
    return transferPolicy().shrinkPart(transport, previous);
}

export function shouldStage(transport: UploadTransport, bytes: number, rate: number): boolean {
    return transferPolicy().shouldStage(transport, bytes, rate);
}

export function shouldAbandonDirect(
    transport: UploadTransport,
    bytes: number,
    loaded: number,
    elapsedMs: number,
): boolean {
    return transferPolicy().shouldAbandonDirect(
        transport,
        bytes,
        loaded,
        Math.max(0, Math.floor(elapsedMs)),
    );
}

export function validatePartCount(
    transport: UploadTransport,
    ciphertextBytes: number,
    partBytes: number,
): void {
    transferPolicy().validatePartCount(transport, ciphertextBytes, partBytes);
}
