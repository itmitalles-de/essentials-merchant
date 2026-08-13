import { bootstrapWorker, PaymentMethod, runMigrations, TransactionalConnection } from '@vendure/core';
import { populateInitialData } from '@vendure/core/cli/populate';
import { initialData } from './initial-data';
import { config } from './vendure-config';

async function initialize(): Promise<void> {
    await runMigrations(config);
    // Initialization only needs Vendure's dependency graph; integration polling belongs to the
    // long-running worker process and would create avoidable network calls here.
    process.env.SHOP_SUITE_INTEGRATION_DISABLED = 'true';
    const worker = await bootstrapWorker(config);
    const connection = worker.app.get(TransactionalConnection).rawConnection;
    if ((await connection.getRepository(PaymentMethod).count()) === 0) {
        await populateInitialData(worker.app, initialData);
    }
    await worker.app.close();
}

initialize().catch(error => {
    console.error(error);
    process.exitCode = 1;
});
