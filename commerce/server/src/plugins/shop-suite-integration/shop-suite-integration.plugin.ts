import { PluginCommonModule, VendurePlugin } from '@vendure/core';
import { IntegrationOutbox } from './integration-outbox.entity';
import { IntegrationWorkerService } from './integration-worker.service';
import { OrderOutboxService } from './order-outbox.service';

@VendurePlugin({
    compatibility: '~3.7.0',
    imports: [PluginCommonModule],
    entities: [IntegrationOutbox],
    providers: [OrderOutboxService, IntegrationWorkerService],
})
export class ShopSuiteIntegrationPlugin {}
