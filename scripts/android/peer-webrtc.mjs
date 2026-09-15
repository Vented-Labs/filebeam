import playwright from 'playwright';
import { createHash } from 'node:crypto';
import { execFile, spawn } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';

const baseURL = process.env.BASE_URL;
const turnURL = process.env.TURN_URL;
const results = process.env.RESULTS_DIR;
const binary = new URL('../../cli/target/release/beam', import.meta.url).pathname;
if (!baseURL || !turnURL || !results) throw new Error('BASE_URL, TURN_URL, and RESULTS_DIR are required');

const run = promisify(execFile);
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const payload = (size, salt) => Buffer.from({ length: size }, (_, i) => (i + salt) & 255);
const privateRoot = await mkdtemp(join(tmpdir(), 'filebeam-peer-webrtc-'));
const evidence = { base_url: baseURL, turn_url: turnURL, cases: [] };

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
    return { context, page: await context.newPage() };
}

async function acceptRisk(page) {
    const dialog = page.getByRole('dialog', { name: 'WebRTC privacy' });
    if (await dialog.isVisible().catch(() => false)) await dialog.getByRole('button', { name: 'Accept and continue' }).click();
}

async function browserSender(page, name, bytes) {
    await page.goto(baseURL);
    await page.locator('#filebeam-picker').setInputFiles({ name, mimeType: 'application/octet-stream', buffer: bytes });
    await page.getByRole('radio', { name: 'WebRTC (live)' }).click();
    await page.getByRole('button', { name: 'Encrypt and share' }).click();
    await acceptRisk(page);
    await page.getByRole('heading', { name: 'Your live transfer is ready' }).waitFor({ timeout: 90_000 });
    return page.locator('#share-link').inputValue();
}

async function browserDownload(page, link, name) {
    await page.goto(link);
    await page.getByText('WebRTC live transfer. The sender must keep their browser open.').waitFor({ timeout: 30_000 });
    const download = page.waitForEvent('download', { timeout: 90_000 });
    await page.getByRole('button', { name: `Download ${name}` }).click();
    await acceptRisk(page);
    return readFile((await (await download).path()));
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
    const bytes = payload(1_048_913, relayOnly ? 71 : 19);
    const name = `${direction}-${relayOnly ? 'turn' : 'direct'}.bin`;
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
        } finally { await sender.context.close(); }
        return;
    }
    const source = join(privateRoot, name);
    await writeFile(source, bytes);
    const sender = await nativeSender(source, relayOnly);
    const receiver = await page(browser, relayOnly);
    try {
        const actual = await browserDownload(receiver.page, sender.link, name);
        const pair = await selectedPair(receiver.page);
        assertPair(pair, relayOnly);
        if (!actual.equals(bytes)) throw new Error(`${direction}: SHA-256 mismatch`);
        evidence.cases.push({ direction, transport: relayOnly ? 'turn-relay' : 'direct', sha256: digest(actual), selected_pair: pair });
    } finally {
        await receiver.context.close();
        if (sender.proc.exitCode === null) sender.proc.kill('SIGINT');
        await new Promise((resolve) => sender.proc.once('exit', resolve));
    }
}

const browser = await playwright.chromium.launch({ headless: true });
try {
    for (const relayOnly of [false, true]) {
        await runDirection(browser, relayOnly, 'web-to-native');
        await runDirection(browser, relayOnly, 'native-to-web');
    }
    await writeFile(join(results, 'peer-webrtc.json'), `${JSON.stringify(evidence, null, 2)}\n`, { mode: 0o600 });
    process.stdout.write(`${JSON.stringify(evidence)}\n`);
} finally {
    await browser.close();
    await rm(privateRoot, { recursive: true, force: true });
}
