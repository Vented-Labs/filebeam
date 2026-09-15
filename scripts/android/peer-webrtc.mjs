import playwright from 'playwright';
import { createHash } from 'node:crypto';
import { execFile, spawn } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import { appendFile, mkdir, mkdtemp, readFile, rm, truncate, writeFile } from 'node:fs/promises';
import { once } from 'node:events';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';

const baseURL = process.env.BASE_URL;
const turnURL = process.env.TURN_URL;
const results = process.env.RESULTS_DIR;
const binary = new URL('../../cli/target/release/beam', import.meta.url).pathname;
const selectedCases = new Set((process.env.PEER_WEBRTC_CASES ?? '').split(',').filter(Boolean));
const largeBytes = Number(process.env.PEER_WEBRTC_LARGE_BYTES ?? 0);
const scratchRoot = process.env.PEER_SCRATCH_ROOT ?? tmpdir();
if (!baseURL || !turnURL || !results) throw new Error('BASE_URL, TURN_URL, and RESULTS_DIR are required');

const run = promisify(execFile);
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const payload = (size, salt) => Buffer.from({ length: size }, (_, i) => (i + salt) & 255);
await mkdir(scratchRoot, { recursive: true });
const privateRoot = await mkdtemp(join(scratchRoot, 'filebeam-peer-webrtc-'));
const evidence = { base_url: baseURL, turn_url: turnURL, cases: [] };
const browserMessages = [];

async function persistEvidence() {
    await writeFile(join(results, 'peer-webrtc.json'), `${JSON.stringify(evidence, null, 2)}\n`, { mode: 0o600 });
}

async function page(browser, relayOnly) {
    const context = await browser.newContext();
    await context.addInitScript((forceRelay) => {
        const Original = window.RTCPeerConnection;
        const peers = [];
        window.__filebeamPeers = peers;
        window.RTCPeerConnection = function (configuration, ...rest) {
            const config = forceRelay ? { ...(configuration ?? {}), iceTransportPolicy: 'relay' } : configuration;
            const peer = new Original(config, ...rest);
            peers.push(peer);
            return peer;
        };
    }, relayOnly);
    const tab = await context.newPage();
    if (largeBytes > 0) {
        const devtools = await context.newCDPSession(tab);
        await devtools.send('Storage.overrideQuotaForOrigin', {
            origin: baseURL,
            quotaSize: largeBytes * 2 + 512 * 1024 * 1024,
        });
    }
    tab.on('console', (message) => browserMessages.push(`console ${message.type()}: ${message.text()}`));
    tab.on('pageerror', (error) => browserMessages.push(`pageerror: ${error.message}`));
    return { context, page: tab };
}

async function acceptRisk(page) {
    const dialog = page.getByRole('dialog', { name: 'WebRTC privacy' });
    if (await dialog.isVisible().catch(() => false)) await dialog.getByRole('button', { name: 'Accept and continue' }).click();
}

async function browserSender(page, name, bytes) {
    try {
        await page.goto(baseURL);
        await page.locator('#filebeam-picker').setInputFiles({ name, mimeType: 'application/octet-stream', buffer: bytes });
        await page.getByRole('radio', { name: 'WebRTC (live)' }).click();
        await page.getByRole('button', { name: 'Encrypt and share' }).click();
        await acceptRisk(page);
        await page.getByRole('heading', { name: 'Your live transfer is ready' }).waitFor({ timeout: 90_000 });
        return page.locator('#share-link').inputValue();
    } catch (error) {
        const body = await page.locator('body').innerText().catch(() => 'page body unavailable');
        await writeFile(join(results, 'browser-failure.txt'), `${browserMessages.join('\n')}\n\n${body}\n`, { mode: 0o600 });
        throw error;
    }
}

async function browserDownload(page, link, name) {
    try {
        await page.goto(link);
        await page.getByText('WebRTC live transfer. The sender must keep their browser open.').waitFor({ timeout: 30_000 });
        const individual = page.getByRole('button', { name: `Download ${name}` });
        await (await individual.count() ? individual : page.getByRole('button', { name: 'Download files' })).click();
        await acceptRisk(page);
        let pair = null;
        for (let attempt = 0; attempt < 20 && !pair; attempt++) {
            pair = await selectedPair(page);
            if (!pair) await page.waitForTimeout(250);
        }
        const completed = page.getByText('1 file downloaded');
        const failed = page.getByRole('alert');
        const timeout = largeBytes > 0 ? 7_200_000 : 180_000;
        await Promise.race([
            completed.waitFor({ timeout }),
            failed.waitFor({ timeout }).then(async () => {
                throw new Error(`browser download failed: ${await failed.innerText()}`);
            }),
        ]);
        return pair;
    } catch (error) {
        const body = await page.locator('body').innerText().catch(() => 'page body unavailable');
        await writeFile(join(results, 'browser-failure.txt'), `${browserMessages.join('\n')}\n\n${body}\n`, { mode: 0o600 });
        throw error;
    }
}

async function opfsWritable(page, label) {
    const hash = createHash('sha256');
    const hostSink = largeBytes > 0 ? createWriteStream(join(privateRoot, `${label}.browser-output`)) : undefined;
    let peakRss = 0;
    await page.exposeBinding('__peerWrite', async (_source, bytes) => {
        const chunk = Buffer.from(bytes);
        hash.update(chunk);
        if (hostSink && !hostSink.write(chunk)) await once(hostSink, 'drain');
        const { stdout } = await run('ps', ['-C', 'chrome-headless-shell', '-o', 'rss=']);
        const rss = stdout.split(/\s+/).reduce((total, value) => total + Number(value || 0), 0);
        peakRss = Math.max(peakRss, rss);
    });
    if (hostSink) {
        await page.exposeBinding('__peerClose', async () => {
            hostSink.end();
            await once(hostSink, 'finish');
        });
    }
    await page.addInitScript((streamToHost) => {
        Object.defineProperty(window, 'showSaveFilePicker', {
            configurable: true,
            value: async () => {
                const root = await navigator.storage.getDirectory();
                const file = await root.getFileHandle(`peer-${crypto.randomUUID()}`, { create: true });
                return {
                    createWritable: async () => {
                        if (streamToHost) {
                            return {
                                write: async (bytes) => window.__peerWrite(bytes),
                                close: async () => window.__peerClose(),
                                abort: async () => window.__peerClose(),
                            };
                        }
                        const writable = await file.createWritable();
                        return {
                            write: async (bytes) => {
                                await window.__peerWrite(bytes);
                                await writable.write(bytes);
                            },
                            close: async () => writable.close(),
                            abort: async () => writable.abort(),
                        };
                    },
                };
            },
        });
    }, Boolean(hostSink));
    return async () => {
        await appendFile(join(results, 'logs', 'browser-rss.tsv'),
            `${label}\tpeak_chromium_rss_kib=${peakRss}\tnode_rss=${process.memoryUsage().rss}\n`);
        return hash.digest('hex');
    };
}

function nativeArgs(home, args, relayOnly) {
    return ['--plain', '--home', home, ...(relayOnly ? ['--webrtc-relay-only'] : ['--accept-peer-address-exposure']), ...args];
}

async function nativeDown(link, output, relayOnly) {
    const home = join(privateRoot, `down-${Math.random()}`);
    await mkdir(home);
    await run(binary, nativeArgs(home, ['down', link, '--output', output], relayOnly), {
        env: { ...process.env, FILEBEAM_INSTANCE: baseURL }, timeout: 120_000,
    });
}

async function nativeSender(source, relayOnly) {
    const home = join(privateRoot, `up-${Math.random()}`);
    await mkdir(home);
    const proc = spawn(binary, nativeArgs(home, ['up', '--transport', 'webrtc', source], relayOnly), {
        env: { ...process.env, FILEBEAM_INSTANCE: baseURL }, stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '', stderr = '';
    proc.stdout.on('data', (data) => { stdout += data; });
    proc.stderr.on('data', (data) => { stderr += data; });
    const link = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`native sender link timeout: ${stderr}`)), 45_000);
        proc.stdout.on('data', (data) => {
            const match = String(data).match(/https?:\/\/\S+/);
            if (match) { clearTimeout(timer); resolve(match[0]); }
        });
        proc.once('exit', (code) => { clearTimeout(timer); reject(new Error(`native sender exited ${code}: ${stderr}`)); });
    });
    return { proc, link, output: () => ({ stdout, stderr }) };
}

async function selectedPair(page) {
    return page.evaluate(async () => {
        for (const peer of window.__filebeamPeers) {
            const stats = await peer.getStats();
            let pair = [...stats.values()].find((item) => item.type === 'transport' && item.selectedCandidatePairId);
            pair = pair && stats.get(pair.selectedCandidatePairId);
            pair ??= [...stats.values()].find((item) => item.type === 'candidate-pair' && item.nominated && item.state === 'succeeded');
            if (!pair) continue;
            const local = stats.get(pair.localCandidateId);
            const remote = stats.get(pair.remoteCandidateId);
            if (local && remote) return {
                local_type: local.candidateType, remote_type: remote.candidateType,
                local_address: local.address ?? null, remote_address: remote.address ?? null,
            };
        }
        return null;
    });
}

function assertPair(pair, relayOnly) {
    if (!pair) throw new Error('browser did not expose a selected ICE candidate pair');
    if (relayOnly && (pair.local_type !== 'relay' || pair.remote_type !== 'relay'))
        throw new Error(`relay-only selected ${pair.local_type}/${pair.remote_type}, not relay/relay`);
    if (relayOnly && [pair.local_address, pair.remote_address].some((address) => address === '127.0.0.1' || address === '::1'))
        throw new Error('relay-only selected a loopback candidate address');
}

async function runDirection(browser, relayOnly, direction) {
    const size = direction === 'native-to-web' && largeBytes > 0 ? largeBytes : 1_048_913;
    const bytes = largeBytes > 0 && direction === 'native-to-web'
        ? undefined
        : payload(size, relayOnly ? 71 : 19);
    const name = `${direction}-${relayOnly ? 'turn' : 'direct'}-${size}.bin`;
    if (direction === 'web-to-native') {
        const sender = await page(browser, relayOnly);
        try {
            const link = await browserSender(sender.page, name, bytes);
            const output = join(privateRoot, `${name}-out`);
            await nativeDown(link, output, relayOnly);
            const pair = await selectedPair(sender.page);
            assertPair(pair, relayOnly);
            const actual = await readFile(join(output, name));
            if (!actual.equals(bytes)) throw new Error(`${direction}: SHA-256 mismatch`);
            evidence.cases.push({ direction, transport: relayOnly ? 'turn-relay' : 'direct', sha256: digest(actual), selected_pair: pair });
            await persistEvidence();
        } finally { await sender.context.close(); }
        return;
    }
    const source = join(privateRoot, name);
    if (largeBytes > 0) {
        await writeFile(source, '');
        await truncate(source, size);
    }
    else await writeFile(source, bytes);
    const sender = await nativeSender(source, relayOnly);
    const receiver = await page(browser, relayOnly);
    try {
        const finishWrite = await opfsWritable(receiver.page, name);
        const pair = await browserDownload(receiver.page, sender.link, name);
        const actualHash = await finishWrite();
        const { stdout } = await run('sha256sum', [source]);
        const sourceHash = stdout.split(/\s+/)[0];
        assertPair(pair, relayOnly);
        if (actualHash !== sourceHash) throw new Error(`${direction}: SHA-256 mismatch`);
        evidence.cases.push({ direction, transport: relayOnly ? 'turn-relay' : 'direct', bytes: size, sha256: actualHash, selected_pair: pair });
        await persistEvidence();
    } finally {
        await receiver.context.close();
        if (sender.proc.exitCode === null) sender.proc.kill('SIGINT');
        await new Promise((resolve) => sender.proc.once('exit', resolve));
    }
}

const browser = await playwright.chromium.launch({
    headless: true,
    env: { ...process.env, TMPDIR: scratchRoot },
});
try {
    for (const relayOnly of [false, true]) {
        for (const direction of ['web-to-native', 'native-to-web']) {
            const name = `${direction}-${relayOnly ? 'turn' : 'direct'}`;
            if (selectedCases.size === 0 || selectedCases.has(name))
                await runDirection(browser, relayOnly, direction);
        }
    }
    await persistEvidence();
    process.stdout.write(`${JSON.stringify(evidence)}\n`);
} finally {
    await browser.close();
    await rm(privateRoot, { recursive: true, force: true });
}
