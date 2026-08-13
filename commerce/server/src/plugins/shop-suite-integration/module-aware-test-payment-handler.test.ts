import assert from 'node:assert/strict';
import test from 'node:test';
import { paymentTestModuleEnabled } from './module-aware-test-payment-handler';

test('payment module check succeeds only on an explicit enabled response', async () => {
    process.env.CORE_API_URL = 'http://core.test:8080';
    process.env.INTEGRATION_SECRET = 'synthetic-integration-secret';
    const enabled = await paymentTestModuleEnabled(async (_request, init) => {
        const headers = new Headers(init?.headers);
        assert.ok(headers.get('x-essentials-signature'));
        return new Response(JSON.stringify({ payment_test_enabled: true }), { status: 200 });
    });
    assert.equal(enabled, true);
});

test('payment module check fails closed on disabled, malformed and unavailable Core', async () => {
    process.env.CORE_API_URL = 'http://core.test:8080';
    process.env.INTEGRATION_SECRET = 'synthetic-integration-secret';
    assert.equal(
        await paymentTestModuleEnabled(async () =>
            new Response(JSON.stringify({ payment_test_enabled: false }), { status: 200 }),
        ),
        false,
    );
    assert.equal(
        await paymentTestModuleEnabled(async () => new Response('nope', { status: 503 })),
        false,
    );
    assert.equal(
        await paymentTestModuleEnabled(async () => {
            throw new Error('synthetic outage');
        }),
        false,
    );
});
