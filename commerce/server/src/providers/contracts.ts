export type PaymentStatus = 'pending' | 'authorized' | 'settled' | 'failed' | 'refunded';
export type ShipmentStatus = 'created' | 'in_transit' | 'delivered' | 'failed';

export interface PaymentRequest {
    idempotencyKey: string;
    orderReference: string;
    amountMinor: number;
    currency: string;
}

export interface PaymentRecord extends PaymentRequest {
    providerId: string;
    status: PaymentStatus;
}

export interface ShipmentRequest {
    idempotencyKey: string;
    orderReference: string;
    carrier: string;
}

export interface ShipmentRecord extends ShipmentRequest {
    providerId: string;
    trackingNumber: string;
    status: ShipmentStatus;
}

export interface ReconciliationDifference {
    reference: string;
    field: 'amountMinor' | 'currency' | 'orderReference' | 'status' | 'carrier' | 'trackingNumber';
    expected: string | number;
    actual: string | number;
}

export interface PaymentProviderPort {
    createPayment(request: PaymentRequest): Promise<PaymentRecord>;
    getPayment(providerId: string): Promise<PaymentRecord | undefined>;
    reconcilePayment(expected: PaymentRecord): Promise<ReconciliationDifference[]>;
}

export interface ShippingProviderPort {
    createShipment(request: ShipmentRequest): Promise<ShipmentRecord>;
    getShipment(providerId: string): Promise<ShipmentRecord | undefined>;
    reconcileShipment(expected: ShipmentRecord): Promise<ReconciliationDifference[]>;
}

export class ProviderContractError extends Error {
    constructor(
        public readonly code:
            | 'invalid_request'
            | 'idempotency_conflict'
            | 'invalid_transition'
            | 'temporary_failure',
        public readonly retryable: boolean,
    ) {
        super(code);
    }
}
