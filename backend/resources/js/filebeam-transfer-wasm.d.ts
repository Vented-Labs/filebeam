declare module '@filebeam/transfer' {
    export default function initialise(
        input?: RequestInfo | URL | Response | BufferSource,
    ): Promise<void>;

    export class AdaptiveConcurrencyController {
        constructor(maximum: number);
        readonly limit: number;
        readonly rate: number;
        sample(key: string, loaded: number, nowMs: number): void;
        forget(key: string): void;
        observe(bytes: number, elapsedMs: number, nowMs: number): void;
        congested(nowMs: number): void;
    }

    export class StageSessionController {
        constructor(id: string, ciphertextBytes: number, checksum: string);
        readonly retries: number;
        reconcile(status: unknown): StageSnapshot;
        acknowledgePart(start: number, bytes: number, status: unknown): StageSnapshot;
        recordRetry(): void;
        reset(id: string): void;
        reprobeAfterRecovery(): boolean;
    }

    export type StageSnapshot = {
        id: string;
        state: 'receiving' | 'finalizing' | 'complete';
        offset: number;
        ciphertext_bytes: number;
        checksum: string;
    };

    export function concurrencyLimit(
        configured: number,
        chunkBytes: number,
        memoryBudget: number,
        platformLimit: number,
    ): number;
    export function retryableStatus(status: number, staging: boolean): boolean;
    export function retryDelayMs(
        attempt: number,
        retryAfterMs: number | undefined,
        jitterMs: number,
    ): number;
    export function chunkCount(plaintextBytes: number, chunkBytes: number): number;
    export function ciphertextBytes(plaintextBytes: number, chunkBytes: number): number;
    export function validateUploadTransport(value: unknown): void;
    export function initialPartBytes(value: unknown, bytesPerMs: number): number;
    export function partBytes(value: unknown, bytesPerMs: number): number;
    export function growPart(
        value: unknown,
        previous: number,
        bytes: number,
        elapsedMs: number,
    ): number;
    export function shrinkPart(value: unknown, previous: number): number;
    export function shouldStage(value: unknown, bytes: number, bytesPerMs: number): boolean;
    export function shouldAbandonDirect(
        value: unknown,
        bytes: number,
        loaded: number,
        elapsedMs: number,
    ): boolean;
    export function validatePartCount(
        value: unknown,
        ciphertextBytes: number,
        partBytes: number,
    ): void;
}
