export function retryDelayMs(attempt: number): number {
    const exponent = Math.min(Math.max(attempt, 1), 10);
    return Math.min(3_600_000, 2 ** exponent * 1_000);
}

export function isRetryableStatus(status: number): boolean {
    return status === 408 || status === 409 || status === 425 || status === 429 || status >= 500;
}

export function shouldApplyProjection(incomingSequence: number, appliedSequence: unknown): boolean {
    const current = Number(appliedSequence);
    return !Number.isFinite(current) || incomingSequence > current;
}
