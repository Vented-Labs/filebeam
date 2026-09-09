import initialize, * as wasm from '@filebeam/encryption';
import { decodeBase64Url, encodeBase64Url } from '../../../../ui/src/lib/base64url';

type EncryptionItem = { id: string; file: Blob; name: string; type: string };
type UploadItem = { id: string; position: number };
type CryptoHasher = wasm.Sha256Hasher;
type LiveItem = { id: string; file: Blob; key: Uint8Array; prefix: Uint8Array; chunkCount: number };

const encoder = new TextEncoder();
const { Sha256Hasher } = wasm;
const waiting = new Map<string, { resolve: () => void; reject: (reason: Error) => void }>();
const cancelled = new Set<string>();
const hashers = new Map<string, CryptoHasher>();
const windows = new Map<string, { limit: number; wake?: () => void }>();
const liveJobs = new Map<
    string,
    {
        transferId: string;
        protocolVersion: number;
        chunkBytes: number;
        items: LiveItem[];
    }
>();
const preparingLiveJobs = new Map<string, LiveItem[]>();
let liveChunkQueue = Promise.resolve();
let initialized: ReturnType<typeof initialize> | undefined;
const context = self as unknown as {
    onmessage: (event: MessageEvent) => Promise<void>;
    postMessage: (message: unknown, transfer?: Transferable[]) => void;
};

function aad(
    protocolVersion: number,
    transferId: string,
    itemId: string,
    index: number | 'manifest' | 'descriptor',
) {
    if (protocolVersion !== 1) throw new Error('Unsupported transfer protocol.');
    return encoder.encode(`filebeam:v${protocolVersion}:${transferId}:${itemId}:${index}`);
}

function isFixedPasswordKdf(value: unknown): boolean {
    if (!value || typeof value !== 'object') return false;
    const kdf = value as Record<string, unknown>;
    return (
        kdf.name === 'argon2id' &&
        kdf.memory_kib === 65536 &&
        kdf.iterations === 3 &&
        kdf.parallelism === 1
    );
}

function ready(): ReturnType<typeof initialize> {
    initialized ??= initialize();
    return initialized;
}

function itemKey(masterKey: Uint8Array, transferId: string, itemId: string): Uint8Array {
    return wasm.derive_item_key(masterKey, transferId, itemId);
}

function reply(message: Record<string, unknown>, transfer?: Transferable[]): void {
    context.postMessage(message, transfer);
}

function clearLiveJob(jobId: string): void {
    const items = liveJobs.get(jobId)?.items ?? preparingLiveJobs.get(jobId);
    if (!items) return;
    for (const item of items) {
        item.key.fill(0);
        item.prefix.fill(0);
    }
    liveJobs.delete(jobId);
    preparingLiveJobs.delete(jobId);
}

context.onmessage = async (event: MessageEvent) => {
    const message = event.data;
    if (message.type === 'uploaded') {
        waiting.get(message.token)?.resolve();
        waiting.delete(message.token);
        return;
    }
    if (message.type === 'cancel') {
        cancelled.add(message.jobId);
        clearLiveJob(message.jobId);
        windows.get(message.jobId)?.wake?.();
        windows.delete(message.jobId);
        for (const hasher of hashers.values()) hasher.free?.();
        hashers.clear();
        for (const [token, waiter] of waiting) {
            if (token.startsWith(`${message.jobId}:`)) {
                waiter.reject(new Error('Upload cancelled.'));
                waiting.delete(token);
            }
        }
        return;
    }
    if (message.type === 'window') {
        const state = windows.get(message.jobId) ?? { limit: 1 };
        state.limit = Math.max(1, Math.min(8, Number(message.limit) || 1));
        state.wake?.();
        windows.set(message.jobId, state);
        return;
    }

    try {
        await ready();
        if (message.type === 'prepare') {
            if (message.password && Array.from(message.password).length < 8) {
                throw new Error('Passwords must contain at least 8 characters.');
            }
            const salt = message.password ? crypto.getRandomValues(new Uint8Array(16)) : null;
            const shareKey = wasm.generate_transfer_key();
            let masterKey = shareKey;
            if (salt) {
                const passwordKey = wasm.derive_password_key(
                    encoder.encode(message.password),
                    salt,
                    65536,
                    3,
                    1,
                );
                masterKey = wasm.derive_password_protected_key(shareKey, passwordKey);
                passwordKey.fill(0);
            }
            reply({
                type: 'prepared',
                jobId: message.jobId,
                requestId: message.requestId,
                shareKey: encodeBase64Url(shareKey),
                masterKey,
                salt: salt && encodeBase64Url(salt),
            });
            return;
        }
        if (message.type === 'hash-start') {
            hashers.get(message.hashId)?.free?.();
            hashers.set(message.hashId, new Sha256Hasher());
            reply({
                type: 'hash-started',
                jobId: message.jobId,
                requestId: message.requestId,
            });
            return;
        }
        if (message.type === 'hash-update') {
            const hasher = hashers.get(message.hashId);
            if (!hasher) throw new Error('Hashing context is unavailable.');
            hasher.update(message.bytes);
            reply(
                {
                    type: 'hash-updated',
                    jobId: message.jobId,
                    requestId: message.requestId,
                    bytes: message.bytes,
                },
                [message.bytes.buffer],
            );
            return;
        }
        if (message.type === 'hash-finalize') {
            const hasher = hashers.get(message.hashId);
            if (!hasher) throw new Error('Hashing context is unavailable.');
            hashers.delete(message.hashId);
            const digest = hasher.finalize();
            reply(
                {
                    type: 'hash-finalized',
                    jobId: message.jobId,
                    requestId: message.requestId,
                    digest,
                },
                [digest.buffer],
            );
            return;
        }
        if (message.type === 'hash-free') {
            hashers.get(message.hashId)?.free?.();
            hashers.delete(message.hashId);
            reply({
                type: 'hash-freed',
                jobId: message.jobId,
                requestId: message.requestId,
            });
            return;
        }
        if (message.type === 'encrypt') {
            const masterKey = message.masterKey as Uint8Array;
            const manifestItems: Array<Record<string, unknown>> = [];
            const descriptorItems: Array<Record<string, unknown>> = [];
            const entries = message.items as EncryptionItem[];
            const serverItems = message.serverItems as UploadItem[];
            const concurrency = Math.max(1, Math.min(8, Number(message.uploadConcurrency) || 4));
            const windowState: { limit: number; wake?: () => void } = { limit: concurrency };
            windows.set(message.jobId, windowState);
            const items: Array<{
                entry: EncryptionItem;
                itemId: string;
                prefix: Uint8Array;
                key: Uint8Array;
                chunkCount: number;
                hasher: CryptoHasher;
            }> = [];
            for (let position = 0; position < entries.length; position++) {
                const entry = entries[position];
                const serverItem = serverItems.find((item) => item.position === position);
                if (!serverItem) throw new Error('Transfer response did not include every item.');
                const prefix = wasm.generate_nonce_prefix();
                const key = itemKey(masterKey, message.transferId, serverItem.id);
                const chunkCount = Math.ceil(entry.file.size / message.chunkBytes) || 1;
                const hasher = new Sha256Hasher();
                items.push({
                    entry,
                    itemId: serverItem.id,
                    prefix,
                    key,
                    chunkCount,
                    hasher,
                });
                manifestItems.push({
                    id: serverItem.id,
                    name: entry.name,
                    type: entry.type,
                    size: entry.file.size,
                    nonce_prefix: encodeBase64Url(prefix),
                    chunk_count: chunkCount,
                    digest: { algorithm: 'sha256', value: '' },
                });
                descriptorItems.push({
                    id: serverItem.id,
                    name: entry.name,
                    type: entry.type,
                    size: entry.file.size,
                    nonce_prefix: encodeBase64Url(prefix),
                    chunk_count: chunkCount,
                });
            }
            if (message.turbo) {
                const descriptorPrefix = wasm.generate_nonce_prefix();
                const descriptor = JSON.stringify({
                    version: 1,
                    purpose: 'turbo-descriptor',
                    chunk_bytes: message.chunkBytes,
                    items: descriptorItems,
                });
                const descriptorCiphertext = wasm.encrypt_manifest(
                    masterKey,
                    descriptorPrefix,
                    encoder.encode(descriptor),
                    aad(message.protocolVersion, message.transferId, 'descriptor', 'descriptor'),
                );
                const token = `${message.jobId}:descriptor`;
                const uploaded = new Promise<void>((resolve, reject) => {
                    waiting.set(token, {
                        resolve,
                        reject,
                    });
                });
                reply({
                    type: 'descriptor',
                    jobId: message.jobId,
                    requestId: message.requestId,
                    encryptedDescriptor: JSON.stringify({
                        v: 1,
                        nonce_prefix: encodeBase64Url(descriptorPrefix),
                        salt: message.salt,
                        ...(message.salt
                            ? {
                                  kdf: {
                                      name: 'argon2id',
                                      memory_kib: 65536,
                                      iterations: 3,
                                      parallelism: 1,
                                  },
                              }
                            : {}),
                        ciphertext: encodeBase64Url(descriptorCiphertext),
                    }),
                    token,
                });
                await uploaded;
            }
            const inFlight = new Set<Promise<void>>();
            let failure: unknown;
            const maximumChunkCount = Math.max(...items.map((item) => item.chunkCount));
            // Read, hash, and encrypt in file order; only upload acknowledgements overlap.
            for (let index = 0; index < maximumChunkCount; index++) {
                for (const item of items) {
                    if (index >= item.chunkCount) continue;
                    if (cancelled.has(message.jobId)) throw new Error('Upload cancelled.');
                    if (failure) throw failure;
                    while (inFlight.size >= windowState.limit) {
                        await Promise.race([
                            Promise.race(inFlight),
                            new Promise<void>((resolve) => {
                                windowState.wake = resolve;
                            }),
                        ]);
                        windowState.wake = undefined;
                    }
                    const start = index * message.chunkBytes;
                    const plaintext = new Uint8Array(
                        await item.entry.file
                            .slice(start, start + message.chunkBytes)
                            .arrayBuffer(),
                    );
                    item.hasher.update(plaintext);
                    const ciphertext = wasm
                        .encrypt_chunk(
                            item.key,
                            item.prefix,
                            index,
                            plaintext,
                            aad(message.protocolVersion, message.transferId, item.itemId, index),
                        )
                        .slice();
                    const token = `${message.jobId}:${item.itemId}:${index}`;
                    const uploaded = new Promise<void>((resolve, reject) => {
                        waiting.set(token, {
                            resolve,
                            reject,
                        });
                    });
                    reply(
                        {
                            type: 'chunk',
                            jobId: message.jobId,
                            requestId: message.requestId,
                            itemId: item.itemId,
                            index,
                            token,
                            ciphertext,
                        },
                        [ciphertext.buffer],
                    );
                    inFlight.add(uploaded);
                    void uploaded.then(
                        () => inFlight.delete(uploaded),
                        (reason) => {
                            failure ??= reason;
                            inFlight.delete(uploaded);
                        },
                    );
                }
            }
            await Promise.all(inFlight);
            windows.delete(message.jobId);
            if (failure) throw failure;
            for (let position = 0; position < manifestItems.length; position++) {
                const digest = items[position].hasher.finalize() as Uint8Array;
                manifestItems[position].digest = {
                    algorithm: 'sha256',
                    value: [...digest].map((byte) => byte.toString(16).padStart(2, '0')).join(''),
                };
            }
            const manifestPrefix = wasm.generate_nonce_prefix();
            const manifest = JSON.stringify({
                version: 1,
                items: manifestItems,
                ...(message.noteTitle ? { title: message.noteTitle } : {}),
                ...(message.readToken ? { read_token: message.readToken } : {}),
                ...(message.noteLanguage ? { language: message.noteLanguage } : {}),
            });
            const ciphertext = wasm.encrypt_manifest(
                masterKey,
                manifestPrefix,
                encoder.encode(manifest),
                aad(message.protocolVersion, message.transferId, 'manifest', 'manifest'),
            );
            reply({
                type: 'finished',
                jobId: message.jobId,
                requestId: message.requestId,
                encryptedManifest: JSON.stringify({
                    v: 1,
                    nonce_prefix: encodeBase64Url(manifestPrefix),
                    salt: message.salt,
                    ...(message.salt
                        ? {
                              kdf: {
                                  name: 'argon2id',
                                  memory_kib: 65536,
                                  iterations: 3,
                                  parallelism: 1,
                              },
                          }
                        : {}),
                    ciphertext: encodeBase64Url(ciphertext),
                }),
            });
            return;
        }
        if (message.type === 'prepare-live') {
            if (message.protocolVersion !== 1) throw new Error('Unsupported transfer protocol.');
            if (!(message.masterKey instanceof Uint8Array))
                throw new Error('Missing transfer key.');
            if (
                !Number.isSafeInteger(message.chunkBytes) ||
                message.chunkBytes < 1 ||
                message.chunkBytes > 25_000_000 - 16
            ) {
                throw new Error('Invalid chunk size.');
            }
            if (
                typeof message.joinToken !== 'string' ||
                !/^[A-Za-z0-9]{64}$/.test(message.joinToken)
            ) {
                throw new Error('Invalid live transfer token.');
            }
            clearLiveJob(message.jobId);
            cancelled.delete(message.jobId);
            const entries = message.items as EncryptionItem[];
            const serverItems = message.serverItems as UploadItem[];
            if (
                !Array.isArray(entries) ||
                !Array.isArray(serverItems) ||
                entries.length !== serverItems.length
            ) {
                throw new Error('Transfer response did not include every item.');
            }
            const manifestItems: Array<Record<string, unknown>> = [];
            const retained: LiveItem[] = [];
            preparingLiveJobs.set(message.jobId, retained);
            for (let position = 0; position < entries.length; position++) {
                const entry = entries[position];
                const serverItem = serverItems.find((item) => item.position === position);
                if (
                    !serverItem ||
                    !entry?.file ||
                    typeof entry.name !== 'string' ||
                    typeof entry.type !== 'string'
                ) {
                    throw new Error('Transfer response did not include every item.');
                }
                const prefix = wasm.generate_nonce_prefix();
                const key = itemKey(message.masterKey, message.transferId, serverItem.id);
                const chunkCount = Math.ceil(entry.file.size / message.chunkBytes) || 1;
                const hasher = new Sha256Hasher();
                let finalized = false;
                retained.push({ id: serverItem.id, file: entry.file, key, prefix, chunkCount });
                try {
                    for (let index = 0; index < chunkCount; index++) {
                        if (cancelled.has(message.jobId)) throw new Error('Transfer cancelled.');
                        const bytes = new Uint8Array(
                            await entry.file
                                .slice(index * message.chunkBytes, (index + 1) * message.chunkBytes)
                                .arrayBuffer(),
                        );
                        if (cancelled.has(message.jobId)) throw new Error('Transfer cancelled.');
                        hasher.update(bytes);
                        reply({
                            type: 'live-preparation-progress',
                            jobId: message.jobId,
                            requestId: message.requestId,
                            itemId: serverItem.id,
                            index,
                            loaded: Math.min(entry.file.size, (index + 1) * message.chunkBytes),
                            total: entry.file.size,
                        });
                    }
                    const digest = hasher.finalize() as Uint8Array;
                    finalized = true;
                    manifestItems.push({
                        id: serverItem.id,
                        name: entry.name,
                        type: entry.type,
                        size: entry.file.size,
                        nonce_prefix: encodeBase64Url(prefix),
                        chunk_count: chunkCount,
                        digest: {
                            algorithm: 'sha256',
                            value: [...digest]
                                .map((byte) => byte.toString(16).padStart(2, '0'))
                                .join(''),
                        },
                    });
                } finally {
                    // finalize consumes the Rust hasher; freeing it again dereferences a null pointer.
                    if (!finalized) hasher.free();
                }
            }
            if (cancelled.has(message.jobId)) throw new Error('Transfer cancelled.');
            const manifestPrefix = wasm.generate_nonce_prefix();
            const manifest = JSON.stringify({
                version: 1,
                items: manifestItems,
                join_token: message.joinToken,
                ...(message.noteTitle ? { title: message.noteTitle } : {}),
                ...(message.readToken ? { read_token: message.readToken } : {}),
                ...(message.noteLanguage ? { language: message.noteLanguage } : {}),
            });
            const ciphertext = wasm.encrypt_manifest(
                message.masterKey,
                manifestPrefix,
                encoder.encode(manifest),
                aad(1, message.transferId, 'manifest', 'manifest'),
            );
            liveJobs.set(message.jobId, {
                transferId: message.transferId,
                protocolVersion: 1,
                chunkBytes: message.chunkBytes,
                items: retained,
            });
            preparingLiveJobs.delete(message.jobId);
            message.masterKey.fill(0);
            reply({
                type: 'live-prepared',
                jobId: message.jobId,
                requestId: message.requestId,
                itemCount: retained.length,
                encryptedManifest: JSON.stringify({
                    v: 1,
                    nonce_prefix: encodeBase64Url(manifestPrefix),
                    salt: message.salt,
                    ...(message.salt
                        ? {
                              kdf: {
                                  name: 'argon2id',
                                  memory_kib: 65536,
                                  iterations: 3,
                                  parallelism: 1,
                              },
                          }
                        : {}),
                    ciphertext: encodeBase64Url(ciphertext),
                }),
            });
            return;
        }
        if (message.type === 'live-chunk') {
            const job = liveJobs.get(message.jobId);
            if (!job || cancelled.has(message.jobId))
                throw new Error('Live transfer is unavailable.');
            if (
                typeof message.itemId !== 'string' ||
                !Number.isSafeInteger(message.index) ||
                message.index < 0
            ) {
                throw new Error('Invalid live chunk request.');
            }
            const item = job.items.find((candidate) => candidate.id === message.itemId);
            if (!item || message.index >= item.chunkCount)
                throw new Error('Invalid live chunk request.');
            const encrypt = async () => {
                if (cancelled.has(message.jobId) || liveJobs.get(message.jobId) !== job) {
                    throw new Error('Transfer cancelled.');
                }
                const plaintext = new Uint8Array(
                    await item.file
                        .slice(message.index * job.chunkBytes, (message.index + 1) * job.chunkBytes)
                        .arrayBuffer(),
                );
                if (cancelled.has(message.jobId) || liveJobs.get(message.jobId) !== job) {
                    plaintext.fill(0);
                    throw new Error('Transfer cancelled.');
                }
                return wasm
                    .encrypt_chunk(
                        item.key,
                        item.prefix,
                        message.index,
                        plaintext,
                        aad(job.protocolVersion, job.transferId, item.id, message.index),
                    )
                    .slice();
            };
            const pending = liveChunkQueue.then(encrypt, encrypt);
            liveChunkQueue = pending.then(
                () => undefined,
                () => undefined,
            );
            const ciphertext = await pending;
            reply(
                {
                    type: 'live-ciphertext',
                    jobId: message.jobId,
                    requestId: message.requestId,
                    itemId: item.id,
                    index: message.index,
                    ciphertext,
                },
                [ciphertext.buffer],
            );
            return;
        }
        if (message.type === 'decrypt-manifest' || message.type === 'decrypt-descriptor') {
            const descriptor = message.type === 'decrypt-descriptor';
            const encrypted = descriptor ? message.encryptedDescriptor : message.encryptedManifest;
            if (typeof encrypted !== 'string' || !encrypted)
                throw new Error(`Missing encrypted ${descriptor ? 'descriptor' : 'manifest'}.`);
            const envelope = JSON.parse(encrypted);
            if (
                envelope?.v !== 1 ||
                envelope.v !== message.protocolVersion ||
                typeof envelope.nonce_prefix !== 'string' ||
                typeof envelope.ciphertext !== 'string'
            )
                throw new Error(`Invalid encrypted ${descriptor ? 'descriptor' : 'manifest'}.`);
            if (
                envelope.salt !== null &&
                envelope.salt !== undefined &&
                typeof envelope.salt !== 'string'
            ) {
                throw new Error('Invalid encrypted manifest salt.');
            }
            if (envelope.salt) {
                if (!isFixedPasswordKdf(envelope.kdf))
                    throw new Error('Unsupported password derivation parameters.');
            } else if (envelope.kdf !== undefined) {
                throw new Error('Invalid encrypted manifest password metadata.');
            }
            let key: Uint8Array;
            if (message.masterKey instanceof Uint8Array) {
                key = message.masterKey;
            } else if (envelope.salt) {
                if (typeof message.key !== 'string' || !message.password)
                    throw new Error('Both the generated key and password are required.');
                const passwordKey = wasm.derive_password_key(
                    encoder.encode(message.password),
                    decodeBase64Url(envelope.salt),
                    65536,
                    3,
                    1,
                );
                key = wasm.derive_password_protected_key(decodeBase64Url(message.key), passwordKey);
                passwordKey.fill(0);
            } else {
                if (typeof message.key !== 'string')
                    throw new Error('A generated decryption key is required.');
                key = decodeBase64Url(message.key);
            }
            const plaintext = wasm.decrypt_manifest(
                key,
                decodeBase64Url(envelope.nonce_prefix),
                decodeBase64Url(envelope.ciphertext),
                aad(
                    message.protocolVersion,
                    message.transferId,
                    descriptor ? 'descriptor' : 'manifest',
                    descriptor ? 'descriptor' : 'manifest',
                ),
            );
            reply({
                type: descriptor ? 'descriptor-manifest' : 'manifest',
                jobId: message.jobId,
                requestId: message.requestId,
                masterKey: key,
                manifest: new TextDecoder().decode(plaintext),
            });
            return;
        }
        if (message.type === 'decrypt-chunk') {
            const key = itemKey(message.masterKey, message.transferId, message.itemId);
            const plaintext = wasm
                .decrypt_chunk(
                    key,
                    message.prefix,
                    message.index,
                    message.ciphertext,
                    aad(message.protocolVersion, message.transferId, message.itemId, message.index),
                )
                .slice();
            reply(
                {
                    type: 'plaintext',
                    jobId: message.jobId,
                    requestId: message.requestId,
                    itemId: message.itemId,
                    index: message.index,
                    plaintext,
                },
                [plaintext.buffer],
            );
        }
    } catch (error) {
        if (message.type === 'prepare-live') clearLiveJob(message.jobId);
        if (message.type === 'prepare-live' && message.masterKey instanceof Uint8Array)
            message.masterKey.fill(0);
        if (message.hashId) hashers.get(message.hashId)?.free?.();
        if (message.hashId) hashers.delete(message.hashId);
        reply({
            type: 'error',
            jobId: message.jobId,
            requestId: message.requestId,
            message: error instanceof Error ? error.message : 'Encryption failed.',
        });
    }
};
