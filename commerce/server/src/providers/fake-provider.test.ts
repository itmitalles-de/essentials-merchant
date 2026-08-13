import assert from 'node:assert/strict';
import test from 'node:test';
import { ProviderContractError } from './contracts';
import { SyntheticProvider } from './fake-provider';
import {
    MemoryReplayStore,
    signProviderWebhook,
    verifyProviderWebhook,
} from './webhook-auth';

test('synthetic payment port enforces idempotency, money and state transitions', async () => {
    const provider = new SyntheticProvider();
    const request = {
        idempotencyKey: 'payment-idempotency-1',
        orderReference: 'ORDER-SYNTHETIC-1',
        amountMinor: 11_900,
        currency: 'EUR',
    };
    const first = await provider.createPayment(request);
    const duplicate = await provider.createPayment(request);
    assert.equal(first.providerId, duplicate.providerId);
    assert.equal(provider.audit.filter(entry => entry.action === 'payment.create').length, 2);
    await assert.rejects(
        provider.createPayment({ ...request, amountMinor: 1 }),
        (error: unknown) => error instanceof ProviderContractError && error.code === 'idempotency_conflict',
    );
    await assert.rejects(
        provider.createPayment({ ...request, idempotencyKey: 'payment-idempotency-2', currency: 'eur' }),
        (error: unknown) => error instanceof ProviderContractError && error.code === 'invalid_request',
    );
    provider.transitionPayment(first.providerId, 'authorized');
    const settled = provider.transitionPayment(first.providerId, 'settled');
    assert.equal(settled.status, 'settled');
    assert.deepEqual(await provider.reconcilePayment(settled), []);
    await assert.rejects(
        Promise.resolve().then(() => provider.transitionPayment(first.providerId, 'authorized')),
        (error: unknown) => error instanceof ProviderContractError && error.code === 'invalid_transition',
    );
});

test('synthetic shipping port maps carrier, tracking and reconciliation states', async () => {
    const provider = new SyntheticProvider();
    const shipment = await provider.createShipment({
        idempotencyKey: 'shipment-idempotency-1',
        orderReference: 'ORDER-SYNTHETIC-2',
        carrier: 'dhl_synthetic',
    });
    assert.match(shipment.trackingNumber, /^TRACK-SYNTHETIC-/);
    const duplicate = await provider.createShipment({
        idempotencyKey: 'shipment-idempotency-1',
        orderReference: 'ORDER-SYNTHETIC-2',
        carrier: 'dhl_synthetic',
    });
    assert.equal(duplicate.providerId, shipment.providerId);
    provider.transitionShipment(shipment.providerId, 'in_transit');
    const delivered = provider.transitionShipment(shipment.providerId, 'delivered');
    assert.deepEqual(await provider.reconcileShipment(delivered), []);
    const difference = await provider.reconcileShipment({ ...delivered, trackingNumber: 'EXPECTED-OTHER' });
    assert.deepEqual(difference.map(item => item.field), ['trackingNumber']);
});

test('temporary provider failures are retryable and leave an audit trail', async () => {
    const provider = new SyntheticProvider();
    provider.failNextRequests();
    await assert.rejects(
        provider.createPayment({
            idempotencyKey: 'payment-idempotency-retry',
            orderReference: 'ORDER-SYNTHETIC-3',
            amountMinor: 100,
            currency: 'EUR',
        }),
        (error: unknown) => error instanceof ProviderContractError && error.retryable,
    );
    assert.equal(provider.audit[0].outcome, 'temporary_failure');
});

test('provider webhooks require a valid signature, fresh timestamp and unused nonce', () => {
    const secret = 'synthetic-provider-webhook-secret-at-least-32-bytes';
    const body = JSON.stringify({ eventId: 'evt_synthetic_1', status: 'settled' });
    const now = 1_800_000_000;
    const headers = signProviderWebhook(secret, body, now, 'nonce-synthetic-0001');
    const store = new MemoryReplayStore();
    assert.doesNotThrow(() => verifyProviderWebhook(secret, body, headers, store, now));
    assert.throws(() => verifyProviderWebhook(secret, body, headers, store, now), /replayed_webhook/);
    assert.throws(
        () => verifyProviderWebhook(secret, `${body} `, headers, new MemoryReplayStore(), now),
        /invalid_webhook/,
    );
    assert.throws(
        () => verifyProviderWebhook(secret, body, headers, new MemoryReplayStore(), now + 301),
        /expired_webhook/,
    );
});
