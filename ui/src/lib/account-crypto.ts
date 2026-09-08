export type AccountKeyBundle = {
    id: number;
    user_id: number;
    version: number;
    public_key: string;
    fingerprint: string;
    custody_mode: 'password' | 'self';
    encrypted_private_key: string | null;
    is_active: boolean;
};

export type RecipientKey = { bundle: AccountKeyBundle; encrypted_key: string };

type GeneratedAccountKey = {
    privateKey: string;
    publicKey: string;
    fingerprint: string;
};

type WorkerResult = WorkerMessage;
const cancellations = new Set<() => void>();

function decode(value: string): Uint8Array {
    try {
        return decodeBase64Url(value);
    } catch {
        throw new Error('Invalid key format.');
    }
}

function requireString(result: WorkerResult, name: string): string {
    if (typeof result[name] !== 'string')
        throw new Error('Account encryption returned an invalid response.');
    return result[name];
}

function request(message: Record<string, unknown>): Promise<WorkerResult> {
    return new Promise((resolve, reject) => {
        const worker = new Worker(
            new URL(
                '../../../backend/resources/js/workers/account-crypto.worker.ts',
                import.meta.url,
            ),
            { type: 'module' },
        );
        const pending = waitForWorkerMessage<WorkerResult>(
            worker,
            (result): result is WorkerResult => 'ok' in result,
            {
                timeoutMs: 120_000,
                timeoutMessage: 'Account encryption is taking too long.',
                workerErrorMessage: 'Account encryption worker failed.',
                messageErrorMessage: 'Account encryption worker returned unreadable data.',
            },
        );
        const cancel = () => pending.reject(new Error('Account encryption was cancelled.'));
        cancellations.add(cancel);
        worker.postMessage(message);
        void pending.promise.then(
            () => {
                worker.terminate();
                cancellations.delete(cancel);
            },
            () => {
                worker.terminate();
                cancellations.delete(cancel);
            },
        );
        pending.promise.then(
            (result) => {
                if (result.ok === true) {
                    resolve(result);

                    return;
                }
                reject(
                    new Error(
                        typeof result.message === 'string'
                            ? result.message
                            : 'Account encryption failed.',
                    ),
                );
            },
            (reason) => reject(reason),
        );
    });
}

export function cancelAccountCrypto(): void {
    while (cancellations.size) cancellations.values().next().value!();
}

export async function generateAccountKey(): Promise<GeneratedAccountKey> {
    const result = await request({ type: 'generate' });
    return {
        privateKey: requireString(result, 'privateKey'),
        publicKey: requireString(result, 'publicKey'),
        fingerprint: requireString(result, 'fingerprint'),
    };
}

export async function createPasswordEnvelope(
    privateKey: string,
    publicKey: string,
    userId: number,
    password: string,
): Promise<string> {
    return requireString(
        await request({ type: 'password-envelope', privateKey, publicKey, userId, password }),
        'envelope',
    );
}

export function exportPrivateKey(privateKey: string): string {
    if (decode(privateKey).length !== 32) throw new Error('Account private key must be 32 bytes.');
    return `fbsk1.${privateKey}`;
}

export async function validatePrivateKey(
    privateExport: string,
    publicKey: string,
): Promise<string> {
    if (!privateExport.startsWith('fbsk1.'))
        throw new Error('A self-custody key must begin with fbsk1.');
    const privateKey = privateExport.slice('fbsk1.'.length);
    if (decode(privateKey).length !== 32) throw new Error('Account private key must be 32 bytes.');
    await request({ type: 'validate-self', privateKey, publicKey });
    return privateKey;
}

async function privateKeyFor(bundle: AccountKeyBundle, secret: string): Promise<string> {
    if (bundle.custody_mode === 'self') return validatePrivateKey(secret, bundle.public_key);
    if (!bundle.encrypted_private_key)
        throw new Error('This account key has no encrypted private key.');
    return requireString(
        await request({
            type: 'unwrap-password',
            envelope: bundle.encrypted_private_key,
            password: secret,
            publicKey: bundle.public_key,
            userId: bundle.user_id,
        }),
        'privateKey',
    );
}

export async function sealRecipientKey(
    masterKey: Uint8Array,
    publicKey: string,
    transferId: string,
    recipientId: number,
    bundleId: number,
): Promise<string> {
    if (masterKey.length !== 32) throw new Error('Transfer key must be 32 bytes.');
    return requireString(
        await request({
            type: 'seal-recipient',
            masterKey: encodeBase64Url(masterKey),
            publicKey,
            transferId,
            recipientId,
            bundleId,
        }),
        'encryptedKey',
    );
}

export async function openRecipientKey(
    recipientKey: RecipientKey,
    secret: string,
    transferId: string,
): Promise<string> {
    const privateKey = await privateKeyFor(recipientKey.bundle, secret);
    return requireString(
        await request({
            type: 'open-recipient',
            privateKey,
            encryptedKey: recipientKey.encrypted_key,
            transferId,
            recipientId: recipientKey.bundle.user_id,
            bundleId: recipientKey.bundle.id,
        }),
        'masterKey',
    );
}
import { decodeBase64Url, encodeBase64Url } from './base64url';
import { waitForWorkerMessage, type WorkerMessage } from './worker-request';
