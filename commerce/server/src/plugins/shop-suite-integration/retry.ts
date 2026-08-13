export interface IntegrationRetryPolicy {
    leaseMs: number;
    retryBaseMs: number;
    retryMaxMs: number;
    maxAttempts: number;
}

export const productionIntegrationPolicy: IntegrationRetryPolicy = {
    leaseMs: 5 * 60_000,
    retryBaseMs: 2_000,
    retryMaxMs: 3_600_000,
    maxAttempts: 20,
};

export function integrationPolicyFromEnvironment(): IntegrationRetryPolicy {
    return {
        leaseMs: boundedEnvironmentNumber('INTEGRATION_LEASE_MS', productionIntegrationPolicy.leaseMs, 250, 86_400_000),
        retryBaseMs: boundedEnvironmentNumber('INTEGRATION_RETRY_BASE_MS', productionIntegrationPolicy.retryBaseMs, 50, 3_600_000),
        retryMaxMs: boundedEnvironmentNumber('INTEGRATION_RETRY_MAX_MS', productionIntegrationPolicy.retryMaxMs, 50, 86_400_000),
        maxAttempts: boundedEnvironmentNumber('INTEGRATION_MAX_ATTEMPTS', productionIntegrationPolicy.maxAttempts, 1, 100),
    };
}

export function retryDelayMs(
    attempt: number,
    policy: IntegrationRetryPolicy = productionIntegrationPolicy,
): number {
    const exponent = Math.min(Math.max(attempt, 1), 10);
    return Math.min(policy.retryMaxMs, policy.retryBaseMs * 2 ** (exponent - 1));
}

export function isRetryableStatus(status: number): boolean {
    return status === 408 || status === 409 || status === 425 || status === 429 || status >= 500;
}

export function shouldApplyProjection(incomingSequence: number, appliedSequence: unknown): boolean {
    const current = Number(appliedSequence);
    return !Number.isFinite(current) || incomingSequence > current;
}

function boundedEnvironmentNumber(name: string, fallback: number, minimum: number, maximum: number): number {
    const parsed = Number(process.env[name] ?? fallback);
    return Number.isFinite(parsed) ? Math.min(maximum, Math.max(minimum, Math.floor(parsed))) : fallback;
}
