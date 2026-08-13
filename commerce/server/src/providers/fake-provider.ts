import {
    PaymentProviderPort,
    PaymentRecord,
    PaymentRequest,
    PaymentStatus,
    ProviderContractError,
    ReconciliationDifference,
    ShipmentRecord,
    ShipmentRequest,
    ShipmentStatus,
    ShippingProviderPort,
} from './contracts';

export interface ProviderAuditEntry {
    action: string;
    reference: string;
    outcome: 'accepted' | 'duplicate' | 'rejected' | 'temporary_failure';
}

export class SyntheticProvider implements PaymentProviderPort, ShippingProviderPort {
    private readonly payments = new Map<string, PaymentRecord>();
    private readonly shipments = new Map<string, ShipmentRecord>();
    private readonly paymentKeys = new Map<string, string>();
    private readonly shipmentKeys = new Map<string, string>();
    private paymentSequence = 0;
    private shipmentSequence = 0;
    private temporaryFailures = 0;
    readonly audit: ProviderAuditEntry[] = [];

    failNextRequests(count = 1): void {
        this.temporaryFailures += count;
    }

    async createPayment(request: PaymentRequest): Promise<PaymentRecord> {
        this.maybeFail(request.orderReference);
        validatePayment(request);
        const existingId = this.paymentKeys.get(request.idempotencyKey);
        if (existingId) {
            const existing = this.payments.get(existingId)!;
            if (
                existing.orderReference !== request.orderReference ||
                existing.amountMinor !== request.amountMinor ||
                existing.currency !== request.currency
            ) {
                this.reject(request.orderReference, 'idempotency_conflict');
            }
            this.audit.push({
                action: 'payment.create',
                reference: request.orderReference,
                outcome: 'duplicate',
            });
            return { ...existing };
        }
        const providerId = `pay_synthetic_${++this.paymentSequence}`;
        const record: PaymentRecord = { ...request, providerId, status: 'pending' };
        this.paymentKeys.set(request.idempotencyKey, providerId);
        this.payments.set(providerId, record);
        this.audit.push({ action: 'payment.create', reference: request.orderReference, outcome: 'accepted' });
        return { ...record };
    }

    async getPayment(providerId: string): Promise<PaymentRecord | undefined> {
        const record = this.payments.get(providerId);
        return record ? { ...record } : undefined;
    }

    transitionPayment(providerId: string, status: PaymentStatus): PaymentRecord {
        const record = this.payments.get(providerId);
        if (!record || !allowedPaymentTransition(record.status, status)) {
            this.reject(providerId, 'invalid_transition');
        }
        record.status = status;
        this.audit.push({ action: 'payment.transition', reference: providerId, outcome: 'accepted' });
        return { ...record };
    }

    async reconcilePayment(expected: PaymentRecord): Promise<ReconciliationDifference[]> {
        const actual = this.payments.get(expected.providerId);
        if (!actual) {
            return [{ reference: expected.providerId, field: 'status', expected: expected.status, actual: 'missing' }];
        }
        return compare(expected, actual, ['amountMinor', 'currency', 'orderReference', 'status']);
    }

    async createShipment(request: ShipmentRequest): Promise<ShipmentRecord> {
        this.maybeFail(request.orderReference);
        validateShipment(request);
        const existingId = this.shipmentKeys.get(request.idempotencyKey);
        if (existingId) {
            const existing = this.shipments.get(existingId)!;
            if (existing.orderReference !== request.orderReference || existing.carrier !== request.carrier) {
                this.reject(request.orderReference, 'idempotency_conflict');
            }
            this.audit.push({
                action: 'shipment.create',
                reference: request.orderReference,
                outcome: 'duplicate',
            });
            return { ...existing };
        }
        const providerId = `ship_synthetic_${++this.shipmentSequence}`;
        const record: ShipmentRecord = {
            ...request,
            providerId,
            trackingNumber: `TRACK-SYNTHETIC-${String(this.shipmentSequence).padStart(6, '0')}`,
            status: 'created',
        };
        this.shipmentKeys.set(request.idempotencyKey, providerId);
        this.shipments.set(providerId, record);
        this.audit.push({ action: 'shipment.create', reference: request.orderReference, outcome: 'accepted' });
        return { ...record };
    }

    async getShipment(providerId: string): Promise<ShipmentRecord | undefined> {
        const record = this.shipments.get(providerId);
        return record ? { ...record } : undefined;
    }

    transitionShipment(providerId: string, status: ShipmentStatus): ShipmentRecord {
        const record = this.shipments.get(providerId);
        if (!record || !allowedShipmentTransition(record.status, status)) {
            this.reject(providerId, 'invalid_transition');
        }
        record.status = status;
        this.audit.push({ action: 'shipment.transition', reference: providerId, outcome: 'accepted' });
        return { ...record };
    }

    async reconcileShipment(expected: ShipmentRecord): Promise<ReconciliationDifference[]> {
        const actual = this.shipments.get(expected.providerId);
        if (!actual) {
            return [{ reference: expected.providerId, field: 'status', expected: expected.status, actual: 'missing' }];
        }
        return compare(expected, actual, ['orderReference', 'status', 'carrier', 'trackingNumber']);
    }

    private maybeFail(reference: string): void {
        if (this.temporaryFailures === 0) return;
        this.temporaryFailures -= 1;
        this.audit.push({ action: 'provider.request', reference, outcome: 'temporary_failure' });
        throw new ProviderContractError('temporary_failure', true);
    }

    private reject(reference: string, code: 'idempotency_conflict' | 'invalid_transition'): never {
        this.audit.push({ action: 'provider.request', reference, outcome: 'rejected' });
        throw new ProviderContractError(code, false);
    }
}

function validatePayment(request: PaymentRequest): void {
    if (
        !validKey(request.idempotencyKey) ||
        !validReference(request.orderReference) ||
        !Number.isSafeInteger(request.amountMinor) ||
        request.amountMinor <= 0 ||
        !/^[A-Z]{3}$/.test(request.currency)
    ) {
        throw new ProviderContractError('invalid_request', false);
    }
}

function validateShipment(request: ShipmentRequest): void {
    if (
        !validKey(request.idempotencyKey) ||
        !validReference(request.orderReference) ||
        !/^[a-z0-9_-]{2,32}$/.test(request.carrier)
    ) {
        throw new ProviderContractError('invalid_request', false);
    }
}

function validKey(value: string): boolean {
    return value.length >= 8 && value.length <= 200 && !/[\r\n]/.test(value);
}

function validReference(value: string): boolean {
    return value.length >= 1 && value.length <= 100 && !/[\r\n]/.test(value);
}

function allowedPaymentTransition(from: PaymentStatus, to: PaymentStatus): boolean {
    const transitions: Record<PaymentStatus, PaymentStatus[]> = {
        pending: ['authorized', 'failed'],
        authorized: ['settled', 'failed'],
        settled: ['refunded'],
        failed: [],
        refunded: [],
    };
    return transitions[from].includes(to);
}

function allowedShipmentTransition(from: ShipmentStatus, to: ShipmentStatus): boolean {
    const transitions: Record<ShipmentStatus, ShipmentStatus[]> = {
        created: ['in_transit', 'failed'],
        in_transit: ['delivered', 'failed'],
        delivered: [],
        failed: [],
    };
    return transitions[from].includes(to);
}

function compare<T extends object, K extends keyof T>(
    expected: T,
    actual: T,
    fields: K[],
): ReconciliationDifference[] {
    return fields.flatMap(field =>
        expected[field] === actual[field]
            ? []
            : [{
                reference: String((expected as { providerId?: string }).providerId ?? 'unknown'),
                field: field as ReconciliationDifference['field'],
                expected: expected[field] as string | number,
                actual: actual[field] as string | number,
            }],
    );
}
