import { Injectable, OnApplicationBootstrap, OnApplicationShutdown } from '@nestjs/common';
import { closeSync, openSync } from 'node:fs';
import { GlobalFlag } from '@vendure/common/lib/generated-types';
import {
    ConfigService,
    Fulfillment,
    FulfillmentService,
    LanguageCode,
    Logger,
    Order,
    OrderService,
    ProcessContext,
    Product,
    ProductService,
    ProductVariant,
    ProductVariantService,
    RequestContext,
    RequestContextService,
    TaxRate,
    TransactionalConnection,
    User,
} from '@vendure/core';
import { In, LessThan, LessThanOrEqual } from 'typeorm';
import { IntegrationOutbox } from './integration-outbox.entity';
import {
    integrationPolicyFromEnvironment,
    isRetryableStatus,
    retryDelayMs,
    shouldApplyProjection,
} from './retry';
import {
    integrationSigningKeyFromEnvironment,
    signIntegrationRequest,
} from './request-auth';

const loggerContext = 'ShopSuiteIntegrationWorker';

interface CoreOutboxEvent {
    id: string;
    sequence: number;
    event_type: 'vendure.product.project' | 'vendure.fulfillment.project';
    aggregate_type: string;
    aggregate_id: string;
    idempotency_key: string;
    payload: Record<string, unknown>;
    attempts: number;
    created_at: string;
}

interface ProductProjection {
    core_id: string;
    sku: string;
    name: string;
    unit: string;
    sales_price_net: number | string;
    vat_rate_percent: number | string;
    available_stock: number | string;
    active: boolean;
}

interface FulfillmentProjection {
    core_order_id: string;
    vendure_order_id: string;
    carrier: string | null;
    tracking_number: string;
}

interface IntegrationAdminCommand {
    id: string;
    provider: string;
    action: 'requeue';
    target_id: string;
    attempts: number;
}

class TargetError extends Error {
    constructor(
        message: string,
        readonly status?: number,
    ) {
        super(message);
    }
}

@Injectable()
export class IntegrationWorkerService implements OnApplicationBootstrap, OnApplicationShutdown {
    private timer?: NodeJS.Timeout;
    private running = false;
    private adminContext?: RequestContext;
    private readonly policy = integrationPolicyFromEnvironment();
    private readonly signingKey = integrationSigningKeyFromEnvironment();

    constructor(
        private readonly processContext: ProcessContext,
        private readonly connection: TransactionalConnection,
        private readonly configService: ConfigService,
        private readonly requestContextService: RequestContextService,
        private readonly productService: ProductService,
        private readonly productVariantService: ProductVariantService,
        private readonly orderService: OrderService,
        private readonly fulfillmentService: FulfillmentService,
    ) {}

    onApplicationBootstrap(): void {
        if (!this.processContext.isWorker || process.env.SHOP_SUITE_INTEGRATION_DISABLED === 'true') {
            return;
        }
        const interval = Math.max(
            250,
            Number(process.env.INTEGRATION_POLL_INTERVAL_MS ?? 1_000),
        );
        this.timer = setInterval(() => void this.tick(), interval);
        void this.tick();
        Logger.info(`Integration polling every ${interval} ms`, loggerContext);
    }

    onApplicationShutdown(): void {
        if (this.timer) {
            clearInterval(this.timer);
        }
    }

    private async tick(): Promise<void> {
        if (this.running) {
            return;
        }
        this.running = true;
        try {
            if (!(await this.commerceModuleEnabled())) {
                return;
            }
            await this.deliverVendureEvents();
            await this.consumeCoreEvents();
            await this.processAdminCommands();
            await this.reportDiagnostics();
        } catch (error) {
            Logger.error(String(error), loggerContext, error instanceof Error ? error.stack : undefined);
        } finally {
            this.running = false;
        }
    }

    private async commerceModuleEnabled(): Promise<boolean> {
        const response = await this.coreFetch('/api/integrations/vendure/module-status', {
            method: 'POST',
            body: JSON.stringify({}),
        });
        if (!response.ok) {
            throw new TargetError(`Core module status returned ${response.status}`, response.status);
        }
        const result = (await response.json()) as { enabled?: boolean };
        return result.enabled === true;
    }

    private async processAdminCommands(): Promise<void> {
        const response = await this.coreFetch('/api/integrations/vendure/commands/claim', {
            method: 'POST',
            body: JSON.stringify({ limit: 10 }),
        });
        if (!response.ok) {
            throw new TargetError(`Core command claim returned ${response.status}`, response.status);
        }
        const commands = (await response.json()) as IntegrationAdminCommand[];
        for (const command of commands) {
            let error: string | undefined;
            try {
                if (command.action !== 'requeue') {
                    throw new Error(`unsupported admin command ${command.action}`);
                }
                const repository = this.connection.rawConnection.getRepository(IntegrationOutbox);
                const event = await repository.findOne({ where: { eventId: command.target_id } });
                if (!event) {
                    throw new Error('target event was not found');
                }
                if (event.status !== 'dead') {
                    throw new Error('target event is no longer dead');
                }
                await repository.update(event.id, {
                    status: 'pending',
                    availableAt: new Date(),
                    lockedAt: null,
                    lastError: 'manually requeued',
                });
            } catch (caught) {
                error = sanitizedError(caught);
            }
            await this.coreFetchChecked(
                `/api/integrations/vendure/commands/${command.id}/complete`,
                {
                    method: 'POST',
                    body: JSON.stringify({ error: error ?? null }),
                },
            );
        }
    }

    private async reportDiagnostics(): Promise<void> {
        const repository = this.connection.rawConnection.getRepository(IntegrationOutbox);
        const events = await repository.find({ order: { createdAt: 'DESC' }, take: 100 });
        const lastSuccessful = events.find(event => event.deliveredAt)?.deliveredAt ?? null;
        const lastFailed = events.find(event => event.lastError)?.lastError ?? null;
        await this.coreFetchChecked('/api/integrations/vendure/diagnostics', {
            method: 'POST',
            body: JSON.stringify({
                health_status: events.some(event => event.status === 'dead') ? 'degraded' : 'healthy',
                last_success_at: lastSuccessful?.toISOString() ?? null,
                last_error: lastFailed ? sanitizedError(lastFailed) : null,
                observed_at: new Date().toISOString(),
                events: events.map(event => ({
                    event_id: event.eventId,
                    event_type: event.eventType,
                    status: event.status,
                    attempts: event.attempts,
                    available_at: event.availableAt?.toISOString() ?? null,
                    locked_at: event.lockedAt?.toISOString() ?? null,
                    last_error: event.lastError ? sanitizedError(event.lastError) : null,
                    created_at: event.createdAt.toISOString(),
                    delivered_at: event.deliveredAt?.toISOString() ?? null,
                })),
            }),
        });
    }

    private async deliverVendureEvents(): Promise<void> {
        for (let processed = 0; processed < 10; processed += 1) {
            const event = await this.claimVendureEvent();
            if (!event) {
                return;
            }
            this.testFailpoint('after_vendure_claim');
            try {
                const response = await this.coreFetch('/api/integrations/vendure/orders', {
                    method: 'POST',
                    headers: { 'idempotency-key': event.eventId },
                    body: JSON.stringify(event.payload),
                });
                if (!response.ok) {
                    throw new TargetError(
                        `Core order import returned ${response.status}: ${await response.text()}`,
                        response.status,
                    );
                }
                await this.completeVendureEvent(event);
            } catch (error) {
                await this.failVendureEvent(event, error);
            }
        }
    }

    private async claimVendureEvent(): Promise<IntegrationOutbox | undefined> {
        const repository = this.connection.rawConnection.getRepository(IntegrationOutbox);
        await repository.update(
            { status: 'processing', lockedAt: LessThan(new Date(Date.now() - this.policy.leaseMs)) },
            { status: 'pending', lockedAt: null, lastError: 'worker lease expired' },
        );
        return this.connection.rawConnection.transaction(async manager => {
            const event = await manager.getRepository(IntegrationOutbox).findOne({
                where: { status: 'pending', availableAt: LessThanOrEqual(new Date()) },
                order: { createdAt: 'ASC' },
                lock: { mode: 'pessimistic_write', onLocked: 'skip_locked' },
            });
            if (!event) {
                return undefined;
            }
            event.status = 'processing';
            event.attempts += 1;
            event.lockedAt = new Date();
            return manager.getRepository(IntegrationOutbox).save(event);
        });
    }

    private async completeVendureEvent(event: IntegrationOutbox): Promise<void> {
        await this.connection.rawConnection.getRepository(IntegrationOutbox).update(event.id, {
            status: 'delivered',
            lockedAt: null,
            lastError: null,
            deliveredAt: new Date(),
        });
    }

    private async failVendureEvent(event: IntegrationOutbox, error: unknown): Promise<void> {
        const targetError = error instanceof TargetError ? error : undefined;
        const retryable =
            targetError?.status === undefined || isRetryableStatus(targetError.status);
        const dead = !retryable || event.attempts >= this.policy.maxAttempts;
        await this.connection.rawConnection.getRepository(IntegrationOutbox).update(event.id, {
            status: dead ? 'dead' : 'pending',
            availableAt: new Date(Date.now() + retryDelayMs(event.attempts, this.policy)),
            lockedAt: null,
            lastError: sanitizedError(error),
        });
    }

    private async consumeCoreEvents(): Promise<void> {
        const response = await this.coreFetch('/api/integrations/vendure/outbox/claim', {
            method: 'POST',
            body: JSON.stringify({ limit: 10 }),
        });
        if (!response.ok) {
            throw new TargetError(`Core outbox claim returned ${response.status}`, response.status);
        }
        const events = (await response.json()) as CoreOutboxEvent[];
        for (const event of events) {
            this.testFailpoint('after_core_claim');
            try {
                if (event.event_type === 'vendure.product.project') {
                    await this.projectProduct(
                        event.payload as unknown as ProductProjection,
                        event.sequence,
                    );
                } else if (event.event_type === 'vendure.fulfillment.project') {
                    await this.projectFulfillment(event.payload as unknown as FulfillmentProjection);
                } else {
                    throw new Error(`Unsupported Core event ${event.event_type}`);
                }
                this.testFailpoint('before_core_ack');
                await this.coreFetchChecked(`/api/integrations/vendure/outbox/${event.id}/ack`, {
                    method: 'POST',
                });
            } catch (error) {
                await this.coreFetchChecked(`/api/integrations/vendure/outbox/${event.id}/retry`, {
                    method: 'POST',
                    body: JSON.stringify({ error: String(error) }),
                });
            }
        }
    }

    private async projectProduct(projection: ProductProjection, sequence: number): Promise<void> {
        const ctx = await this.getAdminContext();
        const variantRepository = this.connection.rawConnection.getRepository(ProductVariant);
        let variant = await variantRepository.findOne({
            where: { sku: projection.sku },
            relations: { product: true },
        });
        const appliedSequence = (variant?.customFields as { coreProjectionSequence?: number } | undefined)
            ?.coreProjectionSequence;
        if (variant && !shouldApplyProjection(sequence, appliedSequence)) {
            Logger.debug(
                `Ignoring stale Core projection ${sequence} for ${projection.sku}`,
                loggerContext,
            );
            return;
        }
        const net = Number(projection.sales_price_net);
        const vat = Number(projection.vat_rate_percent);
        const netCents = Math.round(net * 100);
        const availableStock = Math.max(0, Math.floor(Number(projection.available_stock)));
        const taxRates = await this.connection.rawConnection.getRepository(TaxRate).find({
            relations: { category: true },
        });
        const taxRate = taxRates.find(candidate => Number(candidate.value) === vat);
        if (!taxRate) {
            throw new Error(`Vendure has no tax category for the Core VAT rate ${vat}`);
        }
        const customFields = {
            coreId: projection.core_id,
            coreVatRate: vat,
            coreProjectionSequence: sequence,
        };

        let product: Product;
        if (!variant) {
            product = await this.productService.create(ctx, {
                enabled: projection.active,
                translations: [
                    {
                        languageCode: LanguageCode.de,
                        name: projection.name,
                        slug: slugify(`${projection.sku}-${projection.name}`),
                        description: `${projection.unit} · aus Essentials+ Merchant`,
                    },
                ],
            });
            [variant] = await this.productVariantService.create(ctx, [
                {
                    productId: product.id,
                    sku: projection.sku,
                    enabled: projection.active,
                    price: netCents,
                    taxCategoryId: taxRate.category.id,
                    stockOnHand: availableStock,
                    trackInventory: GlobalFlag.TRUE,
                    customFields,
                    translations: [
                        { languageCode: LanguageCode.de, name: projection.name },
                    ],
                } as never,
            ]);
        } else {
            product = variant.product;
            await this.productService.update(ctx, {
                id: product.id,
                enabled: projection.active,
                translations: [
                    {
                        languageCode: LanguageCode.de,
                        name: projection.name,
                        slug: slugify(`${projection.sku}-${projection.name}`),
                        description: `${projection.unit} · aus Essentials+ Merchant`,
                    },
                ],
            });
            const currentStock = await this.productVariantService.getFulfillableStockLevel(ctx, variant);
            const available = await this.productVariantService.getSaleableStockLevel(ctx, variant);
            const allocated = Math.max(0, currentStock - available);
            [variant] = await this.productVariantService.update(ctx, [
                {
                    id: variant.id,
                    sku: projection.sku,
                    enabled: projection.active,
                    price: netCents,
                    taxCategoryId: taxRate.category.id,
                    stockOnHand: availableStock + allocated,
                    trackInventory: GlobalFlag.TRUE,
                    customFields,
                    translations: [
                        { languageCode: LanguageCode.de, name: projection.name },
                    ],
                } as never,
            ]);
        }

        await this.coreFetchChecked('/api/integrations/vendure/mappings', {
            method: 'POST',
            body: JSON.stringify({
                entity_type: 'article',
                internal_id: projection.core_id,
                external_id: String(variant.id),
                metadata: { product_id: String(product.id), sku: projection.sku },
            }),
        });
    }

    private async projectFulfillment(projection: FulfillmentProjection): Promise<void> {
        const ctx = await this.getAdminContext();
        const order = await this.orderService.findOne(ctx, projection.vendure_order_id, [
            'lines',
            'fulfillments',
        ]);
        if (!order) {
            throw new Error(`Vendure order ${projection.vendure_order_id} was not found`);
        }
        const lineIds = order.lines.map(line => line.id);
        let fulfillment = await this.connection.rawConnection.getRepository(Fulfillment).findOne({
            where: {
                trackingCode: projection.tracking_number,
                lines: { orderLineId: In(lineIds) },
            },
            relations: { lines: true },
        });
        if (fulfillment?.state === 'Shipped' || fulfillment?.state === 'Delivered') {
            return;
        }
        if (!fulfillment) {
            const result = await this.orderService.createFulfillment(ctx, {
                lines: order.lines.map(line => ({
                    orderLineId: line.id,
                    quantity: line.quantity,
                })),
                handler: {
                    code: 'manual-fulfillment',
                    arguments: [
                        { name: 'method', value: projection.carrier ?? 'Essentials+ Merchant' },
                        { name: 'trackingCode', value: projection.tracking_number },
                    ],
                },
            });
            if ('errorCode' in result) {
                throw new Error(result.message);
            }
            fulfillment = result;
        }
        if (!fulfillment) {
            throw new Error('Vendure did not return a fulfillment');
        }
        const fulfillmentId = fulfillment.id;
        if (!order.fulfillments?.some(item => String(item.id) === String(fulfillmentId))) {
            await this.connection.rawConnection
                .getRepository(Order)
                .createQueryBuilder()
                .relation('fulfillments')
                .of(order)
                .add(fulfillment);
        }
        if (fulfillment.state === 'Created') {
            const pending = await this.fulfillmentService.transitionToState(
                ctx,
                fulfillment.id,
                'Pending',
            );
            if ('errorCode' in pending) {
                throw new Error(pending.message);
            }
            fulfillment = pending.fulfillment;
        }
        if (fulfillment.state === 'Pending') {
            const shipped = await this.fulfillmentService.transitionToState(
                ctx,
                fulfillment.id,
                'Shipped',
            );
            if ('errorCode' in shipped) {
                throw new Error(shipped.message);
            }
        }
    }

    private async getAdminContext(): Promise<RequestContext> {
        if (this.adminContext) {
            return this.adminContext;
        }
        const identifier = this.configService.authOptions.superadminCredentials.identifier;
        const user = await this.connection.rawConnection.getRepository(User).findOneOrFail({
            where: { identifier },
            relations: { roles: { channels: true } },
        });
        this.adminContext = await this.requestContextService.create({ apiType: 'admin', user });
        return this.adminContext;
    }

    private coreFetch(path: string, init: RequestInit): Promise<Response> {
        const url = new URL(path, process.env.CORE_API_URL);
        const method = (init.method ?? 'GET').toUpperCase();
        const body = init.body == null ? '' : String(init.body);
        const timeoutMs = Math.min(
            60_000,
            Math.max(250, Number(process.env.INTEGRATION_HTTP_TIMEOUT_MS ?? 10_000)),
        );
        return fetch(url, {
            ...init,
            body: body || undefined,
            signal: AbortSignal.timeout(timeoutMs),
            headers: {
                'content-type': 'application/json',
                ...signIntegrationRequest(this.signingKey, method, url.pathname, body),
                ...init.headers,
            },
        });
    }

    private async coreFetchChecked(path: string, init: RequestInit): Promise<Response> {
        const response = await this.coreFetch(path, init);
        if (!response.ok) {
            throw new TargetError(
                `Core endpoint ${path} returned ${response.status}: ${await response.text()}`,
                response.status,
            );
        }
        return response;
    }

    private testFailpoint(name: string): void {
        if (process.env.APP_ENV !== 'test') {
            return;
        }
        const enabled = new Set(
            (process.env.INTEGRATION_TEST_FAILPOINTS ?? '')
                .split(',')
                .map(value => value.trim())
                .filter(Boolean),
        );
        if (!enabled.has(name)) {
            return;
        }
        try {
            const descriptor = openSync(`/tmp/essentials-integration-failpoint-${name}`, 'wx');
            closeSync(descriptor);
        } catch (error) {
            const code = (error as NodeJS.ErrnoException).code;
            if (code === 'EEXIST') {
                return;
            }
            throw error;
        }
        Logger.warn(`Triggering one-shot test failpoint ${name}`, loggerContext);
        process.exit(70);
    }
}

function sanitizedError(error: unknown): string {
    return String(error)
        .replace(/[\r\n\t]+/g, ' ')
        .slice(0, 512);
}

function slugify(value: string): string {
    return value
        .normalize('NFKD')
        .replace(/[\u0300-\u036f]/g, '')
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/(^-|-$)/g, '');
}
