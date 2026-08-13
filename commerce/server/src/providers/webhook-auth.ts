import { createHmac, timingSafeEqual } from 'node:crypto';

export interface ProviderWebhookHeaders {
    timestamp: string;
    nonce: string;
    signature: string;
}

export class MemoryReplayStore {
    private readonly nonces = new Map<string, number>();

    consume(nonce: string, expiresAt: number, now: number): boolean {
        for (const [knownNonce, expiry] of this.nonces) {
            if (expiry < now) this.nonces.delete(knownNonce);
        }
        if (this.nonces.has(nonce)) return false;
        this.nonces.set(nonce, expiresAt);
        return true;
    }
}

export function signProviderWebhook(
    secret: string,
    rawBody: string,
    timestamp: number,
    nonce: string,
): ProviderWebhookHeaders {
    return {
        timestamp: String(timestamp),
        nonce,
        signature: createHmac('sha256', secret)
            .update(`${timestamp}.${nonce}.${rawBody}`, 'utf8')
            .digest('hex'),
    };
}

export function verifyProviderWebhook(
    secret: string,
    rawBody: string,
    headers: ProviderWebhookHeaders,
    replayStore: MemoryReplayStore,
    now = Math.floor(Date.now() / 1_000),
    maximumSkewSeconds = 300,
): void {
    const timestamp = Number(headers.timestamp);
    if (!Number.isInteger(timestamp) || Math.abs(now - timestamp) > maximumSkewSeconds) {
        throw new Error('expired_webhook');
    }
    if (!/^[A-Za-z0-9-]{8,128}$/.test(headers.nonce) || !/^[0-9a-f]{64}$/.test(headers.signature)) {
        throw new Error('invalid_webhook');
    }
    const expected = Buffer.from(
        createHmac('sha256', secret)
            .update(`${timestamp}.${headers.nonce}.${rawBody}`, 'utf8')
            .digest('hex'),
        'ascii',
    );
    const actual = Buffer.from(headers.signature, 'ascii');
    if (actual.length !== expected.length || !timingSafeEqual(actual, expected)) {
        throw new Error('invalid_webhook');
    }
    if (!replayStore.consume(headers.nonce, timestamp + maximumSkewSeconds, now)) {
        throw new Error('replayed_webhook');
    }
}
