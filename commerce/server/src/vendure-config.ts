import path from 'node:path';
import { AssetServerPlugin } from '@vendure/asset-server-plugin';
import {
    DefaultJobQueuePlugin,
    DefaultSchedulerPlugin,
    DefaultSearchPlugin,
    LanguageCode,
    VendureConfig,
} from '@vendure/core';
import { DashboardPlugin } from '@vendure/dashboard/plugin';
import { GraphiqlPlugin } from '@vendure/graphiql-plugin';
import 'dotenv/config';
import { ShopSuiteIntegrationPlugin } from './plugins/shop-suite-integration/shop-suite-integration.plugin';
import { moduleAwareTestPaymentHandler } from './plugins/shop-suite-integration/module-aware-test-payment-handler';

process.env.VENDURE_DISABLE_TELEMETRY ??= 'true';

const isDevelopment = process.env.APP_ENV === 'dev';
const serverPort = Number(process.env.VENDURE_PORT ?? 3000);

function required(name: string): string {
    const value = process.env[name];
    if (value) {
        return value;
    }
    // Vite evaluates the server config while compiling the static Dashboard, but none of these
    // values are used or embedded in that client bundle. Runtime startup never sets this flag.
    if (process.env.VENDURE_DASHBOARD_BUILD === 'true') {
        return `dashboard-build-only-${name.toLowerCase()}`;
    }
    throw new Error(`${name} must be set`);
}

const cookieSecret = required('COOKIE_SECRET');
const superadminUsername = required('VENDURE_SUPERADMIN_USERNAME');
const superadminPassword = required('VENDURE_SUPERADMIN_PASSWORD');
const databaseHost = required('VENDURE_DB_HOST');
const databaseName = required('VENDURE_DB_NAME');
const databaseUsername = required('VENDURE_DB_USERNAME');
const databasePassword = required('VENDURE_DB_PASSWORD');
required('CORE_API_URL');
required('INTEGRATION_SECRET');
if (Boolean(process.env.INTEGRATION_PREVIOUS_KEY_ID) !== Boolean(process.env.INTEGRATION_PREVIOUS_SECRET)) {
    throw new Error('INTEGRATION_PREVIOUS_KEY_ID and INTEGRATION_PREVIOUS_SECRET must be configured together');
}

export const config: VendureConfig = {
    defaultLanguageCode: LanguageCode.de,
    apiOptions: {
        port: serverPort,
        adminApiPath: 'admin-api',
        shopApiPath: 'shop-api',
        trustProxy: isDevelopment ? false : 1,
        cors: {
            origin: process.env.STOREFRONT_ORIGIN ?? 'http://localhost:3001',
            credentials: true,
        },
        ...(isDevelopment ? { adminApiDebug: true, shopApiDebug: true } : {}),
    },
    authOptions: {
        tokenMethod: ['bearer', 'cookie'],
        superadminCredentials: {
            identifier: superadminUsername,
            password: superadminPassword,
        },
        cookieOptions: { secret: cookieSecret },
    },
    dbConnectionOptions: {
        type: 'postgres',
        synchronize: false,
        migrations: [path.join(__dirname, './migrations/*.+(js|ts)')],
        logging: false,
        host: databaseHost,
        port: Number(process.env.VENDURE_DB_PORT ?? 5432),
        database: databaseName,
        username: databaseUsername,
        password: databasePassword,
    },
    paymentOptions: { paymentMethodHandlers: [moduleAwareTestPaymentHandler] },
    customFields: {
        ProductVariant: [
            { name: 'coreId', type: 'string', nullable: true, internal: true },
            { name: 'coreVatRate', type: 'float', nullable: true, internal: true },
            { name: 'coreProjectionSequence', type: 'int', nullable: true, internal: true },
        ],
    },
    plugins: [
        ...(isDevelopment ? [GraphiqlPlugin.init()] : []),
        AssetServerPlugin.init({
            route: 'assets',
            assetUploadDir: path.join(__dirname, '../static/assets'),
            assetUrlPrefix:
                process.env.ASSET_URL_PREFIX ?? `http://localhost:${serverPort}/assets/`,
        }),
        DefaultSchedulerPlugin.init(),
        DefaultJobQueuePlugin.init({ useDatabaseForBuffer: true }),
        DefaultSearchPlugin.init({ bufferUpdates: false, indexStockStatus: true }),
        ShopSuiteIntegrationPlugin,
        DashboardPlugin.init({
            route: 'dashboard',
            appDir: isDevelopment
                ? path.join(__dirname, '../dist/dashboard')
                : path.join(__dirname, 'dashboard'),
        }),
    ],
};
