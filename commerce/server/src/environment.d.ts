export {};

declare global {
    namespace NodeJS {
        interface ProcessEnv {
            APP_ENV?: string;
            VENDURE_PORT?: string;
            COOKIE_SECRET: string;
            VENDURE_SUPERADMIN_USERNAME: string;
            VENDURE_SUPERADMIN_PASSWORD: string;
            VENDURE_DB_HOST: string;
            VENDURE_DB_PORT?: string;
            VENDURE_DB_NAME: string;
            VENDURE_DB_USERNAME: string;
            VENDURE_DB_PASSWORD: string;
            CORE_API_URL: string;
            INTEGRATION_SECRET: string;
            STOREFRONT_ORIGIN?: string;
            ASSET_URL_PREFIX?: string;
            INTEGRATION_POLL_INTERVAL_MS?: string;
        }
    }
}
