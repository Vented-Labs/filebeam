import initialize, * as wasm from '@filebeam/encryption';
import { decodeBase64Url, encodeBase64Url } from '../../../../ui/src/lib/base64url';

const encoder = new TextEncoder();
let initialized: ReturnType<typeof initialize> | undefined;

function copyBytes(bytes: Uint8Array): Uint8Array<ArrayBuffer> {
    return Uint8Array.from(bytes);
}

function key(value: string, label: string): Uint8Array {
    const bytes = decodeBase64Url(value);
    if (bytes.length !== 32) throw new Error(`${label} must be 32 bytes.`);
    return bytes;
}

function stringValue(message: Record<string, unknown>, name: string): string {
    const value = message[name];
    if (typeof value !== 'string') throw new Error(`Invalid ${name}.`);
    return value;
}

function integerValue(message: Record<string, unknown>, name: string): number {
    const value = message[name];
    if (typeof value !== 'number' || !Number.isInteger(value)) throw new Error(`Invalid ${name}.`);
    return value;
}

async function ready(): Promise<void> {
    initialized ??= initialize();
    await initialized;
}

async function accountWrappingKey(passwordKey: Uint8Array, publicKey: string): Promise<Uint8Array> {
    const imported = await crypto.subtle.importKey('raw', copyBytes(passwordKey), 'HKDF', false, [
        'deriveBits',
    ]);
    const bits = await crypto.subtle.deriveBits(
        {
            name: 'HKDF',
            hash: 'SHA-256',
            salt: new Uint8Array(),
            info: encoder.encode(`filebeam:v1:item-key:account-key-v1:${publicKey}`),
        },
        imported,
        256,
    );
    return new Uint8Array(bits);
}

async function fingerprint(publicKey: Uint8Array): Promise<string> {
    const digest = await crypto.subtle.digest('SHA-256', copyBytes(publicKey));
    return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

async function handle(message: Record<string, unknown>): Promise<Record<string, unknown>> {
    await ready();
    if (message.type === 'generate') {
        const pair = wasm.generate_account_keypair();
        const privateKey = pair.slice(0, 32);
        const publicKey = pair.slice(32);
        pair.fill(0);
        return {
            privateKey: encodeBase64Url(privateKey),
            publicKey: encodeBase64Url(publicKey),
            fingerprint: await fingerprint(publicKey),
        };
    }
    if (message.type === 'password-envelope') {
        const password = stringValue(message, 'password');
        const publicKey = stringValue(message, 'publicKey');
        const userId = integerValue(message, 'userId');
        const privateKey = key(stringValue(message, 'privateKey'), 'Account private key');
        const publicKeyBytes = key(publicKey, 'Account public key');
        if (!password) throw new Error('A password and account are required.');
        const salt = crypto.getRandomValues(new Uint8Array(16));
        const prefix = crypto.getRandomValues(new Uint8Array(16));
        const passwordKey = wasm.derive_password_key(encoder.encode(password), salt, 65536, 3, 1);
        const wrappingKey = await accountWrappingKey(passwordKey, publicKey);
        passwordKey.fill(0);
        const ciphertext = wasm.encrypt_manifest(
            wrappingKey,
            prefix,
            privateKey,
            encoder.encode(`filebeam:account-key:v1:${userId}:${publicKey}`),
        );
        wrappingKey.fill(0);
        privateKey.fill(0);
        publicKeyBytes.fill(0);
        return {
            envelope: JSON.stringify({
                v: 1,
                kdf: {
                    name: 'argon2id',
                    memory_kib: 65536,
                    iterations: 3,
                    parallelism: 1,
                },
                salt: encodeBase64Url(salt),
                nonce_prefix: encodeBase64Url(prefix),
                ciphertext: encodeBase64Url(ciphertext),
            }),
        };
    }
    if (message.type === 'unwrap-password') {
        const envelope = JSON.parse(stringValue(message, 'envelope')) as Record<string, unknown>;
        const password = stringValue(message, 'password');
        const publicKey = stringValue(message, 'publicKey');
        const userId = integerValue(message, 'userId');
        const kdf = envelope.kdf;
        if (
            !password ||
            envelope.v !== 1 ||
            !kdf ||
            typeof kdf !== 'object' ||
            (kdf as Record<string, unknown>).name !== 'argon2id' ||
            (kdf as Record<string, unknown>).memory_kib !== 65536 ||
            (kdf as Record<string, unknown>).iterations !== 3 ||
            (kdf as Record<string, unknown>).parallelism !== 1
        )
            throw new Error('Invalid account key envelope.');
        const salt = decodeBase64Url(stringValue(envelope, 'salt'));
        const prefix = decodeBase64Url(stringValue(envelope, 'nonce_prefix'));
        const ciphertext = decodeBase64Url(stringValue(envelope, 'ciphertext'));
        if (salt.length !== 16 || prefix.length !== 16 || ciphertext.length !== 48)
            throw new Error('Invalid account key envelope.');
        const passwordKey = wasm.derive_password_key(encoder.encode(password), salt, 65536, 3, 1);
        const wrappingKey = await accountWrappingKey(passwordKey, publicKey);
        passwordKey.fill(0);
        const privateKey = wasm.decrypt_manifest(
            wrappingKey,
            prefix,
            ciphertext,
            encoder.encode(`filebeam:account-key:v1:${userId}:${publicKey}`),
        );
        wrappingKey.fill(0);
        if (privateKey.length !== 32) throw new Error('Invalid account private key.');
        return { privateKey: encodeBase64Url(privateKey) };
    }
    if (message.type === 'validate-self') {
        const privateKey = key(stringValue(message, 'privateKey'), 'Account private key');
        const publicKey = key(stringValue(message, 'publicKey'), 'Account public key');
        const challenge = crypto.getRandomValues(new Uint8Array(32));
        const envelope = wasm.seal_key_for_recipient(
            publicKey,
            challenge,
            encoder.encode('filebeam:account-key:v1:validation'),
        );
        const opened = wasm.open_recipient_envelope(
            privateKey,
            envelope,
            encoder.encode('filebeam:account-key:v1:validation'),
        );
        if (
            opened.length !== challenge.length ||
            !opened.every((byte, index) => byte === challenge[index])
        )
            throw new Error('The private key does not match this account key.');
        privateKey.fill(0);
        return { valid: true };
    }
    if (message.type === 'seal-recipient') {
        const masterKey = key(stringValue(message, 'masterKey'), 'Transfer key');
        const publicKey = key(stringValue(message, 'publicKey'), 'Recipient public key');
        const transferId = stringValue(message, 'transferId');
        const recipientId = integerValue(message, 'recipientId');
        const bundleId = integerValue(message, 'bundleId');
        const aad = `filebeam:recipient:v1:${transferId}:${recipientId}:${bundleId}`;
        const encryptedKey = wasm.seal_key_for_recipient(publicKey, masterKey, encoder.encode(aad));
        masterKey.fill(0);
        return { encryptedKey: encodeBase64Url(encryptedKey) };
    }
    if (message.type === 'open-recipient') {
        const privateKey = key(stringValue(message, 'privateKey'), 'Account private key');
        const envelope = decodeBase64Url(stringValue(message, 'encryptedKey'));
        const transferId = stringValue(message, 'transferId');
        const recipientId = integerValue(message, 'recipientId');
        const bundleId = integerValue(message, 'bundleId');
        const aad = `filebeam:recipient:v1:${transferId}:${recipientId}:${bundleId}`;
        const masterKey = wasm.open_recipient_envelope(privateKey, envelope, encoder.encode(aad));
        privateKey.fill(0);
        return { masterKey: encodeBase64Url(masterKey) };
    }
    throw new Error('Unsupported account crypto operation.');
}

self.onmessage = async (event: MessageEvent<Record<string, unknown>>) => {
    try {
        const result = await handle(event.data);
        self.postMessage({ ok: true, ...result });
    } catch (error) {
        self.postMessage({
            ok: false,
            message: error instanceof Error ? error.message : 'Account encryption failed.',
        });
    }
};
