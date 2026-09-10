import { expect, test } from '@playwright/test';
import { AdaptiveConcurrency, initialiseTransferPolicy } from '../../ui/src/lib/transfer';

test.beforeEach(async () => initialiseTransferPolicy());

function withClock(run: (set: (milliseconds: number) => void) => void): void {
    const original = Date.now;
    let now = 0;
    Date.now = () => now;
    try {
        run((milliseconds) => {
            now = milliseconds;
        });
    } finally {
        Date.now = original;
    }
}

test('policy learns sampled throughput without speculative growth, then grows after completed requests', () => {
    withClock((set) => {
        const adaptive = new AdaptiveConcurrency(3);
        adaptive.sample('a', 0);
        adaptive.sample('a', 600_000);
        adaptive.sample('b', 0);
        adaptive.sample('c', 0);
        set(2_000);
        adaptive.sample('b', 600_000);
        adaptive.sample('c', 600_000);
        expect(adaptive.limit).toBe(1);
        adaptive.observe(1_000_000, 10);
        adaptive.observe(1_000_000, 10);
        expect(adaptive.limit).toBe(2);
    });
});

test('policy keeps one slot at 100 KiB/s', () => {
    withClock((set) => {
        const adaptive = new AdaptiveConcurrency(3);
        adaptive.sample('a', 0);
        adaptive.sample('a', 100_000);
        adaptive.sample('b', 0);
        set(2_000);
        adaptive.sample('b', 200_000);
        expect(adaptive.limit).toBe(1);
    });
});

test('an aggregate sample without gain rolls back a completed-request probe', () => {
    withClock((set) => {
        const adaptive = new AdaptiveConcurrency(3);
        adaptive.observe(1_000_000, 10);
        adaptive.observe(1_000_000, 10);
        expect(adaptive.limit).toBe(2);
        adaptive.sample('a', 0);
        adaptive.sample('a', 600_000);
        adaptive.sample('b', 0);
        adaptive.sample('c', 0);
        set(2_000);
        adaptive.sample('b', 600_000);
        adaptive.sample('c', 600_000);
        expect(adaptive.limit).toBe(1);
        set(9_999);
        adaptive.observe(1_000_000, 10);
        adaptive.observe(1_000_000, 10);
        expect(adaptive.limit).toBe(1);
    });
});

test('two completed fast requests grow to two without old slow-request assumptions', () => {
    withClock((set) => {
        const adaptive = new AdaptiveConcurrency(3);
        adaptive.observe(1_000_000, 10);
        set(10);
        adaptive.observe(1_000_000, 10);
        expect(adaptive.limit).toBe(2);
    });
});

test('small sub-ms completion samples retain a nonzero scheduler duration', () => {
    withClock((set) => {
        const adaptive = new AdaptiveConcurrency(3);
        adaptive.observe(1_040, 0.05);
        set(1);
        adaptive.observe(1_040, 0.05);
        expect(adaptive.limit).toBe(2);
    });
});

test('completed slow requests do not probe', () => {
    const adaptive = new AdaptiveConcurrency(3);
    adaptive.observe(100_000, 1_000);
    adaptive.observe(100_000, 1_000);
    expect(adaptive.limit).toBe(1);
});

test('congestion halves an active limit', () => {
    const adaptive = new AdaptiveConcurrency(4);
    adaptive.observe(1_000_000, 10);
    adaptive.observe(1_000_000, 10);
    adaptive.congested();
    expect(adaptive.limit).toBe(1);
});
