import assert from 'node:assert/strict';
import test from 'node:test';
import { createHmac } from 'node:crypto';
import { canonicalIntegrationRequest, signIntegrationRequest } from './request-auth';

test('integration request signing covers all replay-sensitive fields', () => {
    const key = { id: 'synthetic-current', secret: 'synthetic-current-secret-123456' };
    const headers = signIntegrationRequest(
        key,
        'POST',
        '/api/integrations/vendure/orders',
        '{"order":"synthetic"}',
        1_700_000_000,
        'nonce-1234567890',
    );
    const expected = createHmac('sha256', key.secret)
        .update(
            canonicalIntegrationRequest(
                'POST',
                '/api/integrations/vendure/orders',
                1_700_000_000,
                'nonce-1234567890',
                '{"order":"synthetic"}',
            ),
        )
        .digest('hex');
    assert.equal(headers['x-essentials-key-id'], key.id);
    assert.equal(headers['x-essentials-signature'], expected);
    assert.notEqual(
        expected,
        signIntegrationRequest(
            key,
            'POST',
            '/api/integrations/vendure/orders',
            '{"order":"changed"}',
            1_700_000_000,
            'nonce-1234567890',
        )['x-essentials-signature'],
    );
});
