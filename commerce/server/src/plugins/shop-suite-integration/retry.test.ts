import assert from 'node:assert/strict';
import test from 'node:test';
import { isRetryableStatus, retryDelayMs, shouldApplyProjection } from './retry';

test('retry delay is exponential and capped', () => {
    assert.equal(retryDelayMs(1), 2_000);
    assert.equal(retryDelayMs(4), 16_000);
    assert.equal(retryDelayMs(99), 1_024_000);
    assert.equal(
        retryDelayMs(4, { leaseMs: 500, retryBaseMs: 50, retryMaxMs: 200, maxAttempts: 3 }),
        200,
    );
});

test('temporary target failures remain retryable', () => {
    assert.equal(isRetryableStatus(409), true);
    assert.equal(isRetryableStatus(503), true);
    assert.equal(isRetryableStatus(422), false);
});

test('late product projections cannot overwrite newer Core state', () => {
    assert.equal(shouldApplyProjection(41, 42), false);
    assert.equal(shouldApplyProjection(42, 42), false);
    assert.equal(shouldApplyProjection(43, 42), true);
    assert.equal(shouldApplyProjection(1, null), true);
});
