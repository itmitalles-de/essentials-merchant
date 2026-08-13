import { createHash, createHmac, randomUUID } from 'node:crypto';

export interface IntegrationSigningKey {
    id: string;
    secret: string;
}

export function integrationSigningKeyFromEnvironment(): IntegrationSigningKey {
    const secret = process.env.INTEGRATION_SECRET;
    if (!secret) {
        throw new Error('INTEGRATION_SECRET must be set');
    }
    return {
        id: process.env.INTEGRATION_KEY_ID ?? 'current',
        secret,
    };
}

export function canonicalIntegrationRequest(
    method: string,
    path: string,
    timestamp: number,
    nonce: string,
    body: string,
): string {
    const bodyHash = createHash('sha256').update(body, 'utf8').digest('hex');
    return `${method.toUpperCase()}\n${path}\n${timestamp}\n${nonce}\n${bodyHash}`;
}

export function signIntegrationRequest(
    key: IntegrationSigningKey,
    method: string,
    path: string,
    body: string,
    timestamp = Math.floor(Date.now() / 1_000),
    nonce: string = randomUUID(),
): Record<string, string> {
    const signature = createHmac('sha256', key.secret)
        .update(canonicalIntegrationRequest(method, path, timestamp, nonce, body), 'utf8')
        .digest('hex');
    return {
        'x-essentials-key-id': key.id,
        'x-essentials-timestamp': String(timestamp),
        'x-essentials-nonce': nonce,
        'x-essentials-signature': signature,
    };
}
