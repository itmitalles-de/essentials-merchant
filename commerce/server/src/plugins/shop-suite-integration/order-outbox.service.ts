import { Injectable, OnApplicationBootstrap } from '@nestjs/common';
import {
    EventBus,
    OrderService,
    PaymentStateTransitionEvent,
    ProcessContext,
    TransactionalConnection,
} from '@vendure/core';
import { IntegrationOutbox } from './integration-outbox.entity';

@Injectable()
export class OrderOutboxService implements OnApplicationBootstrap {
    constructor(
        private readonly processContext: ProcessContext,
        private readonly eventBus: EventBus,
        private readonly orderService: OrderService,
        private readonly connection: TransactionalConnection,
    ) {}

    onApplicationBootstrap(): void {
        if (!this.processContext.isServer) {
            return;
        }
        this.eventBus.registerBlockingEventHandler({
            event: PaymentStateTransitionEvent,
            id: 'shop-suite-order-outbox',
            handler: async event => {
                if (event.toState === 'Authorized' || event.toState === 'Settled') {
                    await this.persist(event);
                }
            },
        });
    }

    private async persist(event: PaymentStateTransitionEvent): Promise<void> {
        const order = await this.orderService.findOne(event.ctx, event.order.id, [
            'customer',
            'lines',
            'lines.productVariant',
            'lines.productVariant.translations',
        ]);
        if (!order?.customer) {
            throw new Error(`Paid Vendure order ${event.order.id} has no customer`);
        }

        const eventId = `payment:${event.payment.id}:${event.toState}`;
        const repository = this.connection.getRepository(event.ctx, IntegrationOutbox);
        const payload = {
            event_id: eventId,
            event_type: 'vendure.order.payment',
            occurred_at: new Date().toISOString(),
            order_id: String(order.id),
            order_code: order.code,
            order_state: order.state,
            currency_code: order.currencyCode,
            customer: {
                id: String(order.customer.id),
                first_name: order.customer.firstName,
                last_name: order.customer.lastName,
                email: order.customer.emailAddress,
                phone: order.customer.phoneNumber ?? '',
            },
            shipping_address: {
                street_line1: order.shippingAddress?.streetLine1 ?? '',
                street_line2: order.shippingAddress?.streetLine2 ?? '',
                postal_code: order.shippingAddress?.postalCode ?? '',
                city: order.shippingAddress?.city ?? '',
                country_code: order.shippingAddress?.countryCode ?? 'DE',
            },
            lines: order.lines.map(line => {
                const customFields = line.productVariant.customFields as {
                    coreVatRate?: number | null;
                };
                return {
                    id: String(line.id),
                    sku: line.productVariant.sku,
                    description: line.productVariant.name || line.productVariant.sku,
                    quantity: String(line.quantity),
                    unit_price_gross_cents: line.unitPriceWithTax,
                    vat_rate_percent: String(customFields.coreVatRate ?? line.taxRate),
                };
            }),
        };

        await repository.save(
            new IntegrationOutbox({
                source: 'vendure',
                eventId,
                eventType: 'vendure.order.payment',
                payload,
                status: 'pending',
                attempts: 0,
                availableAt: new Date(),
                lockedAt: null,
                lastError: null,
                deliveredAt: null,
            }),
        );
    }
}
