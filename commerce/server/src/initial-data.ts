import { InitialData, LanguageCode } from '@vendure/core';
import { moduleAwareTestPaymentHandler } from './plugins/shop-suite-integration/module-aware-test-payment-handler';

export const initialData: InitialData = {
    defaultLanguage: LanguageCode.de,
    defaultZone: 'Deutschland',
    countries: [{ code: 'DE', name: 'Deutschland', zone: 'Deutschland' }],
    taxRates: [
        { name: 'Standard-Umsatzsteuer', percentage: 19 },
        { name: 'Ermäßigte Umsatzsteuer', percentage: 7 },
        { name: 'Steuerfrei', percentage: 0 },
    ],
    shippingMethods: [{ name: 'Standardversand', price: 490, taxRate: 19 }],
    paymentMethods: [
        {
            name: 'Testzahlung',
            handler: {
                code: moduleAwareTestPaymentHandler.code,
                arguments: [{ name: 'automaticSettle', value: 'true' }],
            },
        },
    ],
    collections: [],
};
