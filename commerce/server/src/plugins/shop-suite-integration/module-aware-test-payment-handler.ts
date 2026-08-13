import { randomUUID } from 'node:crypto';
import { LanguageCode, PaymentMethodHandler } from '@vendure/core';
import {
    integrationSigningKeyFromEnvironment,
    signIntegrationRequest,
} from './request-auth';

const statusPath = '/api/integrations/vendure/module-status';

interface ModuleStatus {
    payment_test_enabled?: boolean;
}

export async function paymentTestModuleEnabled(
    fetchImplementation: typeof fetch = fetch,
): Promise<boolean> {
    const coreApiUrl = process.env.CORE_API_URL;
    if (!coreApiUrl) {
        return false;
    }
    const body = '{}';
    const controller = new AbortController();
    const timeout = setTimeout(
        () => controller.abort(),
        Math.max(250, Number(process.env.INTEGRATION_REQUEST_TIMEOUT_MS ?? 5_000)),
    );
    try {
        const response = await fetchImplementation(new URL(statusPath, coreApiUrl), {
            method: 'POST',
            headers: {
                'content-type': 'application/json',
                ...signIntegrationRequest(
                    integrationSigningKeyFromEnvironment(),
                    'POST',
                    statusPath,
                    body,
                ),
            },
            body,
            signal: controller.signal,
        });
        if (!response.ok) {
            return false;
        }
        const status = (await response.json()) as ModuleStatus;
        return status.payment_test_enabled === true;
    } catch {
        return false;
    } finally {
        clearTimeout(timeout);
    }
}

async function requirePaymentTestModule(): Promise<true | string> {
    return (await paymentTestModuleEnabled())
        ? true
        : 'The Essentials+ Merchant test-payment module is disabled or unavailable';
}

/**
 * Synthetic development payment handler. It deliberately preserves the existing handler code so
 * existing Vendure payment-method rows remain compatible, while enforcing the Core-owned module
 * switch on every write operation.
 */
export const moduleAwareTestPaymentHandler = new PaymentMethodHandler({
    code: 'dummy-payment-handler',
    description: [
        {
            languageCode: LanguageCode.en,
            value: 'Synthetic payment provider for automated tests and local development only.',
        },
    ],
    args: {
        automaticSettle: {
            type: 'boolean',
            label: [{ languageCode: LanguageCode.en, value: 'Authorize and settle in one step' }],
            required: true,
            defaultValue: false,
        },
    },
    createPayment: async (_ctx, _order, amount, args, metadata) => {
        const permitted = await requirePaymentTestModule();
        if (permitted !== true) {
            return {
                amount,
                state: 'Error' as const,
                errorMessage: permitted,
                metadata: { errorMessage: permitted },
            };
        }
        if (metadata.shouldDecline) {
            return {
                amount,
                state: 'Declined' as const,
                metadata: { errorMessage: 'Synthetic decline' },
            };
        }
        if (metadata.shouldError) {
            return {
                amount,
                state: 'Error' as const,
                errorMessage: 'Synthetic payment error',
                metadata: { errorMessage: 'Synthetic payment error' },
            };
        }
        return {
            amount,
            state: args.automaticSettle ? ('Settled' as const) : ('Authorized' as const),
            transactionId: randomUUID(),
            metadata,
        };
    },
    settlePayment: async () => {
        const permitted = await requirePaymentTestModule();
        return permitted === true ? { success: true } : { success: false, errorMessage: permitted };
    },
    cancelPayment: async () => {
        const permitted = await requirePaymentTestModule();
        return permitted === true
            ? { success: true, metadata: { cancellationDate: new Date().toISOString() } }
            : { success: false, errorMessage: permitted };
    },
});
