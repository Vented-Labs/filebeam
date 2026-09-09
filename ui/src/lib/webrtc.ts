const MAX_CIPHERTEXT_BYTES = 25_000_000;
const CHUNK_OVERHEAD_BYTES = 16;
const FRAME_BYTES = 16;
const FRAME_PAYLOAD_BYTES = 16 * 1024;
const FRAME_MAGIC = 0x46424348;
const POLL_MS = 1800;
const SDP_TIMEOUT_MS = 10_000;
const CONNECT_TIMEOUT_MS = 30_000;
const REQUEST_TIMEOUT_MS = 120_000;
const CONTROL_LIMIT = 1024;

type Session = {
    id: string;
    offer: RTCSessionDescriptionInit | null;
    status: string;
    progress: number;
};
type ApiResponse<T> = { data: T };
type Description = { type: 'offer' | 'answer'; sdp: string };
type Item = { id: string; chunk_count: number; ciphertext_bytes: number };

export type WebRtcSender = { close(): void };
export type WebRtcReceiver = {
    readChunk(
        itemId: string,
        index: number,
        expectedBytes: number,
        onProgress?: (loaded: number) => void,
    ): Promise<ArrayBuffer>;
    report(
        status: 'active' | 'completed' | 'cancelled' | 'failed',
        progress: number,
    ): Promise<void>;
    close(): void;
    sessionToken: string;
};

function endpoint(transferId: string, suffix = ''): string {
    return `/api/v1/transfers/${encodeURIComponent(transferId)}/webrtc${suffix}`;
}

function abortError(message = 'WebRTC transfer cancelled.'): Error {
    return new DOMException(message, 'AbortError');
}

function validId(value: unknown): value is string {
    return typeof value === 'string' && value.length > 0 && value.length <= 256;
}

function abortable(
    signal: AbortSignal,
    start: (finish: (error?: Error) => void) => void | (() => void),
): Promise<void> {
    if (signal.aborted) return Promise.reject(abortError());
    return new Promise((resolve, reject) => {
        let cleanup: (() => void) | undefined;
        let settled = false;
        const abort = () => finish(abortError());
        const finish = (error?: Error) => {
            if (settled) return;
            settled = true;
            cleanup?.();
            signal.removeEventListener('abort', abort);
            if (error) reject(error);
            else resolve();
        };
        signal.addEventListener('abort', abort, { once: true });
        const result = start(finish);
        if (typeof result === 'function') cleanup = result;
        if (settled) cleanup?.();
    });
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
    return abortable(signal, (finish) => {
        const timeout = window.setTimeout(finish, ms);
        return () => window.clearTimeout(timeout);
    });
}

async function api<T>(
    url: string,
    init: RequestInit,
    signal: AbortSignal,
): Promise<{ status: number; body?: T }> {
    if (signal.aborted) throw abortError();
    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), 15_000);
    const abort = () => controller.abort();
    signal.addEventListener('abort', abort, { once: true });
    try {
        const response = await fetch(url, {
            ...init,
            signal: controller.signal,
            headers: { Accept: 'application/json', ...init.headers },
        });
        if (response.status === 204) return { status: 204 };
        if (!response.ok) return { status: response.status };
        return { status: response.status, body: (await response.json()) as T };
    } finally {
        window.clearTimeout(timeout);
        signal.removeEventListener('abort', abort);
    }
}

async function gathered(pc: RTCPeerConnection, signal: AbortSignal): Promise<Description> {
    await abortable(signal, (finish) => {
        const timeout = window.setTimeout(
            () => done(new Error('Timed out gathering ICE candidates.')),
            SDP_TIMEOUT_MS,
        );
        const change = () => {
            if (pc.iceGatheringState === 'complete') done();
        };
        const done = (error?: Error) => {
            window.clearTimeout(timeout);
            pc.removeEventListener('icegatheringstatechange', change);
            finish(error);
        };
        pc.addEventListener('icegatheringstatechange', change);
        change();
        return () => {
            window.clearTimeout(timeout);
            pc.removeEventListener('icegatheringstatechange', change);
        };
    });
    const description = pc.localDescription;
    if (!description?.sdp || !/a=candidate:/m.test(description.sdp)) {
        throw new Error('No ICE candidates were gathered.');
    }
    return { type: description.type as 'offer' | 'answer', sdp: description.sdp };
}

function control(channel: RTCDataChannel, value: Record<string, unknown>): void {
    const text = JSON.stringify(value);
    if (text.length > CONTROL_LIMIT) throw new Error('WebRTC control frame is too large.');
    channel.send(text);
}

function expectedCiphertextBytes(
    item: Item,
    index: number,
    chunkBytes: number,
): number | undefined {
    if (
        !Number.isSafeInteger(item.chunk_count) ||
        item.chunk_count < 1 ||
        !Number.isSafeInteger(item.ciphertext_bytes)
    )
        return;
    const plaintextBytes = item.ciphertext_bytes - item.chunk_count * CHUNK_OVERHEAD_BYTES;
    if (plaintextBytes < 0 || index < 0 || index >= item.chunk_count) return;
    const plaintext = Math.min(chunkBytes, Math.max(0, plaintextBytes - index * chunkBytes));
    if (index < item.chunk_count - 1 && plaintext !== chunkBytes) return;
    const ciphertext = plaintext + CHUNK_OVERHEAD_BYTES;
    return ciphertext >= CHUNK_OVERHEAD_BYTES && ciphertext <= MAX_CIPHERTEXT_BYTES
        ? ciphertext
        : undefined;
}

function waitForChannelOpen(channel: RTCDataChannel, signal: AbortSignal): Promise<void> {
    if (channel.readyState === 'open') return Promise.resolve();
    return abortable(signal, (finish) => {
        const timeout = window.setTimeout(
            () => done(new Error('Timed out opening WebRTC channel.')),
            CONNECT_TIMEOUT_MS,
        );
        const open = () => done();
        const failed = () => done(new Error('WebRTC channel failed.'));
        const done = (error?: Error) => {
            window.clearTimeout(timeout);
            channel.removeEventListener('open', open);
            channel.removeEventListener('close', failed);
            channel.removeEventListener('error', failed);
            finish(error);
        };
        channel.addEventListener('open', open, { once: true });
        channel.addEventListener('close', failed, { once: true });
        channel.addEventListener('error', failed, { once: true });
        return () => {
            window.clearTimeout(timeout);
            channel.removeEventListener('open', open);
            channel.removeEventListener('close', failed);
            channel.removeEventListener('error', failed);
        };
    });
}

function configureSenderChannel(
    channel: RTCDataChannel,
    items: Map<string, Item>,
    chunkBytes: number,
    framePayloadBytes: number,
    readChunk: (itemId: string, index: number) => Promise<Uint8Array>,
    signal: AbortSignal,
    fail: (error: Error) => void,
): void {
    let busy = false;
    let acknowledge: { seq: number; finish: (error?: Error) => void } | undefined;
    channel.bufferedAmountLowThreshold = framePayloadBytes * 2;
    const stop = () => {
        acknowledge?.finish(abortError());
        channel.close();
    };
    signal.addEventListener('abort', stop, { once: true });
    const waitBuffer = () =>
        abortable(signal, (finish) => {
            const timeout = window.setTimeout(
                () => done(new Error('WebRTC channel remained congested.')),
                REQUEST_TIMEOUT_MS,
            );
            const low = () => done();
            const closed = () => done(new Error('WebRTC channel closed.'));
            const done = (error?: Error) => {
                window.clearTimeout(timeout);
                channel.removeEventListener('bufferedamountlow', low);
                channel.removeEventListener('close', closed);
                finish(error);
            };
            channel.addEventListener('bufferedamountlow', low, { once: true });
            channel.addEventListener('close', closed, { once: true });
            return () => {
                window.clearTimeout(timeout);
                channel.removeEventListener('bufferedamountlow', low);
                channel.removeEventListener('close', closed);
            };
        });
    channel.addEventListener(
        'close',
        () => {
            acknowledge?.finish(new Error('WebRTC channel closed.'));
            signal.removeEventListener('abort', stop);
        },
        { once: true },
    );
    channel.addEventListener('message', (event) => {
        if (typeof event.data !== 'string' || event.data.length > CONTROL_LIMIT) {
            channel.close();
            return;
        }
        let value: Record<string, unknown>;
        try {
            const parsed: unknown = JSON.parse(event.data);
            if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error();
            value = parsed as Record<string, unknown>;
        } catch {
            channel.close();
            return;
        }
        const pendingAcknowledgement = acknowledge;
        if (
            pendingAcknowledgement &&
            value.type === 'ack' &&
            value.seq === pendingAcknowledgement.seq
        ) {
            pendingAcknowledgement.finish();
            return;
        }
        if (
            busy ||
            value.type !== 'request' ||
            !Number.isSafeInteger(value.seq) ||
            (value.seq as number) < 0 ||
            (value.seq as number) > 0xffffffff ||
            !validId(value.itemId) ||
            !Number.isSafeInteger(value.index)
        ) {
            channel.close();
            return;
        }
        const item = items.get(value.itemId);
        const index = value.index as number;
        const expected = item && expectedCiphertextBytes(item, index, chunkBytes);
        if (!item || expected === undefined) {
            channel.close();
            return;
        }
        busy = true;
        void (async () => {
            try {
                const ciphertext = await readChunk(value.itemId as string, index);
                if (!(ciphertext instanceof Uint8Array) || ciphertext.byteLength !== expected) {
                    throw new Error('Encrypted chunk length does not match its declaration.');
                }
                control(channel, {
                    type: 'chunk',
                    seq: value.seq,
                    itemId: value.itemId,
                    index,
                    length: expected,
                });
                for (let offset = 0; offset < ciphertext.byteLength; offset += framePayloadBytes) {
                    while (channel.bufferedAmount > framePayloadBytes * 4) await waitBuffer();
                    if (signal.aborted || channel.readyState !== 'open') throw abortError();
                    const length = Math.min(framePayloadBytes, ciphertext.byteLength - offset);
                    const frame = new Uint8Array(FRAME_BYTES + length);
                    const header = new DataView(frame.buffer);
                    header.setUint32(0, FRAME_MAGIC);
                    header.setUint32(4, value.seq as number);
                    header.setUint32(8, offset);
                    header.setUint32(12, length);
                    frame.set(ciphertext.subarray(offset, offset + length), FRAME_BYTES);
                    channel.send(frame);
                }
                await abortable(signal, (finish) => {
                    const timeout = window.setTimeout(
                        () => done(new Error('WebRTC chunk acknowledgement timed out.')),
                        REQUEST_TIMEOUT_MS,
                    );
                    const done = (error?: Error) => {
                        window.clearTimeout(timeout);
                        acknowledge = undefined;
                        finish(error);
                    };
                    acknowledge = { seq: value.seq as number, finish: done };
                    return () => {
                        window.clearTimeout(timeout);
                        acknowledge = undefined;
                    };
                });
            } catch (error) {
                if (!signal.aborted)
                    fail(
                        error instanceof Error
                            ? error
                            : new Error('Unable to read encrypted chunk.'),
                    );
                channel.close();
            } finally {
                busy = false;
            }
        })();
    });
}

export async function startWebRtcSender(options: {
    transferId: string;
    uploadToken: string;
    signal: AbortSignal;
    readChunk: (itemId: string, index: number) => Promise<Uint8Array>;
    items: Array<{ id: string; chunk_count: number; ciphertext_bytes: number }>;
    chunkBytes: number;
    onActivity?: (message: string) => void;
    onSessions?: (sessions: Array<{ id: string; status: string; progress: number }>) => void;
    onError?: (error: Error) => void;
    onEnded?: (error: Error) => void;
}): Promise<WebRtcSender> {
    if (
        !Number.isSafeInteger(options.chunkBytes) ||
        options.chunkBytes < 1 ||
        options.chunkBytes > MAX_CIPHERTEXT_BYTES
    )
        throw new Error('Invalid chunk size.');
    const items = new Map<string, Item>();
    for (const item of options.items) {
        if (
            !validId(item.id) ||
            items.has(item.id) ||
            expectedCiphertextBytes(item, item.chunk_count - 1, options.chunkBytes) === undefined
        )
            throw new Error('Invalid live item declaration.');
        items.set(item.id, item);
    }
    if (options.signal.aborted) throw abortError();
    const controller = new AbortController();
    const signal = controller.signal;
    const peers = new Map<string, { pc?: RTCPeerConnection; terminal: boolean }>();
    const stop = () => close();
    const close = (reason?: Error) => {
        if (signal.aborted) return;
        controller.abort();
        for (const peer of peers.values()) peer.pc?.close();
        peers.clear();
        options.signal.removeEventListener('abort', stop);
        if (reason) options.onEnded?.(reason);
    };
    options.signal.addEventListener('abort', stop, { once: true });
    const negotiate = async (session: Session, iceServers: RTCIceServer[]) => {
        const state = peers.get(session.id);
        if (!state || state.terminal || !session.offer) return;
        const pc = new RTCPeerConnection({ iceServers });
        state.pc = pc;
        let disconnected: number | undefined;
        const connectionTimeout = window.setTimeout(() => {
            terminal();
            options.onError?.(new Error('The recipient could not establish a WebRTC connection.'));
        }, CONNECT_TIMEOUT_MS);
        const terminal = () => {
            if (state.terminal) return;
            state.terminal = true;
            window.clearTimeout(connectionTimeout);
            if (disconnected) window.clearTimeout(disconnected);
            pc.close();
        };
        pc.addEventListener('connectionstatechange', () => {
            if (pc.connectionState === 'connected') window.clearTimeout(connectionTimeout);
            if (pc.connectionState === 'disconnected')
                disconnected = window.setTimeout(terminal, 15_000);
            else if (disconnected) {
                window.clearTimeout(disconnected);
                disconnected = undefined;
            }
            if (pc.connectionState === 'failed' || pc.connectionState === 'closed') terminal();
        });
        let configuredChannel = false;
        pc.addEventListener('datachannel', (event) => {
            if (configuredChannel) {
                event.channel.close();
                return;
            }
            configuredChannel = true;
            const channel = event.channel;
            if (
                channel.label !== 'filebeam' ||
                !channel.ordered ||
                channel.maxRetransmits !== null ||
                channel.maxPacketLifeTime !== null
            ) {
                channel.close();
                return;
            }
            const configure = () => {
                const maximum = pc.sctp?.maxMessageSize || FRAME_PAYLOAD_BYTES + FRAME_BYTES;
                const payload = Math.min(FRAME_PAYLOAD_BYTES, maximum - FRAME_BYTES);
                if (maximum < CONTROL_LIMIT || payload < 256) {
                    channel.close();
                    options.onError?.(new Error('WebRTC message size is too small.'));
                    return;
                }
                configureSenderChannel(
                    channel,
                    items,
                    options.chunkBytes,
                    payload,
                    options.readChunk,
                    signal,
                    (error) => options.onError?.(error),
                );
            };
            if (channel.readyState === 'open') configure();
            else channel.addEventListener('open', configure, { once: true });
        });
        try {
            await pc.setRemoteDescription(session.offer);
            await pc.setLocalDescription(await pc.createAnswer());
            const answer = await gathered(pc, signal);
            const response = await api(
                endpoint(options.transferId, `/sessions/${encodeURIComponent(session.id)}/answer`),
                {
                    method: 'PUT',
                    headers: {
                        'X-Filebeam-Upload-Token': options.uploadToken,
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify({ description: answer }),
                },
                signal,
            );
            if (response.status !== 204) throw new Error('Unable to publish WebRTC answer.');
            options.onActivity?.('Receiver connected.');
        } catch (error) {
            terminal();
            if (!signal.aborted)
                options.onError?.(
                    error instanceof Error ? error : new Error('WebRTC negotiation failed.'),
                );
        }
    };
    void (async () => {
        let failures = 0;
        while (!signal.aborted) {
            try {
                const response = await api<
                    ApiResponse<{ sessions: Session[]; ice_servers: RTCIceServer[] }>
                >(
                    endpoint(options.transferId, '/sessions'),
                    { headers: { 'X-Filebeam-Upload-Token': options.uploadToken } },
                    signal,
                );
                if ([401, 403, 404, 410].includes(response.status)) {
                    close(new Error('This live share has ended or is no longer authorized.'));
                    return;
                }
                const data = response.body?.data;
                if (!data || !Array.isArray(data.sessions) || !Array.isArray(data.ice_servers))
                    throw new Error('Unable to poll WebRTC sessions.');
                failures = 0;
                const currentSessions = new Set(data.sessions.map((session) => session.id));
                for (const [id, peer] of peers) {
                    if (!currentSessions.has(id)) {
                        peer.pc?.close();
                        peers.delete(id);
                    }
                }
                options.onSessions?.(
                    data.sessions.map(({ id, status, progress }) => ({ id, status, progress })),
                );
                for (const session of data.sessions) {
                    if (!validId(session.id)) continue;
                    if (['completed', 'cancelled', 'failed'].includes(session.status)) {
                        peers.get(session.id)?.pc?.close();
                        peers.delete(session.id);
                        continue;
                    }
                    if (
                        !session.offer ||
                        peers.has(session.id) ||
                        [...peers.values()].filter((peer) => peer.pc && !peer.terminal).length >= 8
                    )
                        continue;
                    peers.set(session.id, { terminal: false });
                    void negotiate(session, data.ice_servers);
                }
            } catch (error) {
                if (signal.aborted) return;
                if (++failures >= 4)
                    options.onError?.(
                        error instanceof Error
                            ? error
                            : new Error('WebRTC session polling failed.'),
                    );
            }
            try {
                await sleep(POLL_MS, signal);
            } catch {
                return;
            }
        }
    })();
    return { close };
}

export async function connectWebRtcReceiver(options: {
    transferId: string;
    joinToken: string;
    signal: AbortSignal;
}): Promise<WebRtcReceiver> {
    if (options.signal.aborted) throw abortError();
    const controller = new AbortController();
    const signal = controller.signal;
    let sessionId = '';
    let sessionToken = '';
    let channel: RTCDataChannel | undefined;
    let pc: RTCPeerConnection | undefined;
    let readQueue = Promise.resolve();
    let closed = false;
    let rejectRead: ((error: Error) => void) | undefined;
    const abort = () => close();
    const close = () => {
        if (closed) return;
        closed = true;
        controller.abort();
        rejectRead?.(abortError());
        channel?.close();
        pc?.close();
        options.signal.removeEventListener('abort', abort);
    };
    options.signal.addEventListener('abort', abort, { once: true });
    const report = async (
        status: 'active' | 'completed' | 'cancelled' | 'failed',
        progress: number,
    ) => {
        const terminal = status !== 'active';
        if (!sessionId || (closed && !terminal)) return;
        const value = Math.round(
            Math.max(0, Math.min(100, Number.isFinite(progress) ? progress : 0)),
        );
        const response = await api(
            endpoint(options.transferId, `/sessions/${encodeURIComponent(sessionId)}`),
            {
                method: 'PATCH',
                headers: {
                    'X-Filebeam-Session-Token': sessionToken,
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({ status, progress: value }),
                keepalive: terminal,
            },
            terminal ? AbortSignal.timeout(5_000) : signal,
        );
        if (response.status !== 204) throw new Error('Unable to update WebRTC session.');
    };
    try {
        const registered = await api<
            ApiResponse<{ id: string; token: string; ice_servers: RTCIceServer[] }>
        >(
            endpoint(options.transferId, '/sessions'),
            {
                method: 'POST',
                headers: {
                    'X-Filebeam-Join-Token': options.joinToken,
                    'Content-Type': 'application/json',
                },
                body: '{}',
            },
            signal,
        );
        const registration = registered.body?.data;
        if (
            registered.status !== 201 ||
            !registration ||
            !validId(registration.id) ||
            !validId(registration.token) ||
            !Array.isArray(registration.ice_servers)
        )
            throw new Error('Unable to register WebRTC receiver.');
        sessionId = registration.id;
        sessionToken = registration.token;
        pc = new RTCPeerConnection({ iceServers: registration.ice_servers });
        channel = pc.createDataChannel('filebeam', { ordered: true });
        channel.binaryType = 'arraybuffer';
        if (channel.maxRetransmits !== null || channel.maxPacketLifeTime !== null)
            throw new Error('WebRTC channel is not reliable.');
        let disconnected: number | undefined;
        pc.addEventListener('connectionstatechange', () => {
            if (pc?.connectionState === 'disconnected')
                disconnected = window.setTimeout(close, 15_000);
            else if (disconnected) {
                window.clearTimeout(disconnected);
                disconnected = undefined;
            }
            if (pc && (pc.connectionState === 'failed' || pc.connectionState === 'closed')) close();
        });
        await pc.setLocalDescription(await pc.createOffer());
        const offer = await gathered(pc, signal);
        const published = await api(
            endpoint(options.transferId, `/sessions/${encodeURIComponent(sessionId)}/offer`),
            {
                method: 'PUT',
                headers: {
                    'X-Filebeam-Session-Token': sessionToken,
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({ description: offer }),
            },
            signal,
        );
        if (published.status !== 204) throw new Error('Unable to publish WebRTC offer.');
        const started = Date.now();
        while (!signal.aborted && Date.now() - started < CONNECT_TIMEOUT_MS) {
            const response = await api<ApiResponse<{ answer: RTCSessionDescriptionInit | null }>>(
                endpoint(options.transferId, `/sessions/${encodeURIComponent(sessionId)}`),
                { headers: { 'X-Filebeam-Session-Token': sessionToken } },
                signal,
            );
            if (response.status === 404) throw new Error('WebRTC sender is unavailable.');
            const data = response.body?.data;
            if (!data) throw new Error('Unable to poll WebRTC answer.');
            if (data.answer) {
                await pc.setRemoteDescription(data.answer);
                break;
            }
            await sleep(POLL_MS, signal);
        }
        if (!pc.remoteDescription) throw new Error('Timed out waiting for WebRTC sender.');
        await waitForChannelOpen(channel, signal);
        const receiver: WebRtcReceiver = {
            sessionToken,
            close,
            report,
            readChunk: (itemId, index, expectedBytes, onProgress) => {
                const run = async (): Promise<ArrayBuffer> => {
                    if (
                        !validId(itemId) ||
                        !Number.isSafeInteger(index) ||
                        index < 0 ||
                        !Number.isSafeInteger(expectedBytes) ||
                        expectedBytes < CHUNK_OVERHEAD_BYTES ||
                        expectedBytes > MAX_CIPHERTEXT_BYTES ||
                        !channel ||
                        channel.readyState !== 'open'
                    )
                        throw new Error('Invalid WebRTC chunk request.');
                    const seq = crypto.getRandomValues(new Uint32Array(1))[0];
                    let bytes: Uint8Array | undefined;
                    let received = 0;
                    return new Promise<ArrayBuffer>((resolve, reject) => {
                        let settled = false;
                        const finish = (error?: Error) => {
                            if (settled) return;
                            settled = true;
                            window.clearTimeout(timeout);
                            signal.removeEventListener('abort', aborted);
                            channel?.removeEventListener('message', message);
                            channel?.removeEventListener('close', channelClosed);
                            rejectRead = undefined;
                            if (error) {
                                reject(error);
                                close();
                            } else resolve(bytes!.buffer as ArrayBuffer);
                        };
                        const reset = () => {
                            window.clearTimeout(timeout);
                            timeout = window.setTimeout(
                                () => finish(new Error('WebRTC chunk request timed out.')),
                                REQUEST_TIMEOUT_MS,
                            );
                        };
                        const aborted = () => finish(abortError());
                        const channelClosed = () => finish(new Error('WebRTC channel closed.'));
                        const message = (event: MessageEvent) => {
                            try {
                                if (typeof event.data === 'string') {
                                    if (event.data.length > CONTROL_LIMIT)
                                        throw new Error('Invalid WebRTC control frame.');
                                    const parsed: unknown = JSON.parse(event.data);
                                    if (
                                        !parsed ||
                                        typeof parsed !== 'object' ||
                                        Array.isArray(parsed)
                                    )
                                        throw new Error('Invalid WebRTC control frame.');
                                    const value = parsed as Record<string, unknown>;
                                    if (
                                        bytes ||
                                        value.type !== 'chunk' ||
                                        value.seq !== seq ||
                                        value.itemId !== itemId ||
                                        value.index !== index ||
                                        value.length !== expectedBytes
                                    )
                                        throw new Error('Unexpected WebRTC chunk response.');
                                    bytes = new Uint8Array(expectedBytes);
                                    reset();
                                    return;
                                }
                                if (
                                    !(event.data instanceof ArrayBuffer) ||
                                    !bytes ||
                                    event.data.byteLength <= FRAME_BYTES ||
                                    event.data.byteLength > FRAME_BYTES + FRAME_PAYLOAD_BYTES
                                )
                                    throw new Error('Invalid WebRTC binary frame.');
                                const frame = new Uint8Array(event.data);
                                const header = new DataView(event.data);
                                const length = header.getUint32(12);
                                const offset = header.getUint32(8);
                                if (
                                    header.getUint32(0) !== FRAME_MAGIC ||
                                    header.getUint32(4) !== seq ||
                                    length === 0 ||
                                    length !== frame.byteLength - FRAME_BYTES ||
                                    offset !== received ||
                                    offset + length > bytes.byteLength
                                )
                                    throw new Error('Invalid WebRTC frame bounds.');
                                bytes.set(frame.subarray(FRAME_BYTES), offset);
                                received += length;
                                onProgress?.(received);
                                if (received === bytes.byteLength) {
                                    control(channel!, { type: 'ack', seq });
                                    finish();
                                } else reset();
                            } catch (error) {
                                finish(
                                    error instanceof Error
                                        ? error
                                        : new Error('Invalid WebRTC response.'),
                                );
                                channel?.close();
                            }
                        };
                        let timeout = window.setTimeout(
                            () => finish(new Error('WebRTC chunk request timed out.')),
                            REQUEST_TIMEOUT_MS,
                        );
                        rejectRead = finish;
                        signal.addEventListener('abort', aborted, { once: true });
                        const dataChannel = channel;
                        if (!dataChannel) {
                            finish(new Error('WebRTC channel closed.'));
                            return;
                        }
                        dataChannel.addEventListener('message', message);
                        dataChannel.addEventListener('close', channelClosed, { once: true });
                        control(dataChannel, { type: 'request', seq, itemId, index });
                    });
                };
                const result = readQueue.then(run, run);
                readQueue = result.then(
                    () => undefined,
                    () => undefined,
                );
                return result;
            },
        };
        void (async () => {
            while (!signal.aborted) {
                try {
                    await sleep(POLL_MS, signal);
                    const heartbeat = await api(
                        endpoint(options.transferId, `/sessions/${encodeURIComponent(sessionId)}`),
                        { headers: { 'X-Filebeam-Session-Token': sessionToken } },
                        signal,
                    );
                    if (heartbeat.status === 404) {
                        close();
                        return;
                    }
                } catch {
                    if (!signal.aborted) close();
                    return;
                }
            }
        })();
        return receiver;
    } catch (error) {
        await report('failed', 0).catch(() => undefined);
        close();
        throw error;
    }
}
