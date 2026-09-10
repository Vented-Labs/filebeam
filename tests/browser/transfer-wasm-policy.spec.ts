import { expect, test } from '@playwright/test';
import {
    AdaptiveConcurrency,
    initialiseTransferPolicy,
    retryDelayMilliseconds,
    transferPolicy,
} from '../../ui/src/lib/transfer';
import { StageSession } from '../../ui/src/lib/transfer-wasm-policy';

test('WASM policy matches the native transfer trace for scheduling, retry, and staging', async () => {
    await initialiseTransferPolicy();
    const now = Date.now;
    let clock = 0;
    Date.now = () => clock;
    try {
        const controller = new AdaptiveConcurrency(3);
        controller.sample('a', 0);
        controller.sample('a', 600_000);
        controller.sample('b', 0);
        controller.sample('c', 0);
        clock = 2_000;
        controller.sample('b', 600_000);
        controller.sample('c', 600_000);
        expect(controller.limit).toBe(1);
        controller.observe(1_000_000, 10);
        controller.observe(1_000_000, 10);
        expect(controller.limit).toBe(2);
        const small = new AdaptiveConcurrency(2);
        small.observe(1_040, 0.05);
        small.observe(1_040, 0.05);
        expect(small.limit).toBe(2);
        expect(transferPolicy().concurrencyLimit(8, 25_000_000 - 16, 128 * 1024 * 1024, 8)).toBe(2);
        expect(retryDelayMilliseconds(2, undefined, 1)).toBe(1_000);

        const session = new StageSession('stage', 8, 'checksum');
        expect(
            session.acknowledgePart(0, 4, {
                id: 'stage',
                state: 'receiving',
                offset: 4,
                ciphertext_bytes: 8,
                checksum: 'checksum',
            }),
        ).toMatchObject({ offset: 4, retries: 0 });
        expect(
            session.reconcile({
                id: 'stage',
                state: 'complete',
                offset: 8,
                ciphertext_bytes: 8,
                checksum: 'checksum',
            }),
        ).toMatchObject({ state: 'complete', offset: 8 });
    } finally {
        Date.now = now;
    }
});
