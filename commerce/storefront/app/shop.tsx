'use client';

import { FormEvent, useCallback, useEffect, useState } from 'react';
import { formatMoney, graphQlError, GraphQlEnvelope } from '@/lib/shop-api';

interface SearchItem {
    productVariantId: string;
    productName: string;
    productVariantName: string;
    sku: string;
    priceWithTax: {
        __typename: 'SinglePrice' | 'PriceRange';
        value?: number;
        min?: number;
        max?: number;
    };
    currencyCode: string;
}

interface Order {
    id: string;
    code: string;
    state: string;
    totalWithTax: number;
    currencyCode: string;
    lines: Array<{ id: string; quantity: number; linePriceWithTax: number; productVariant: { name: string; sku: string } }>;
}

interface CustomerForm {
    firstName: string;
    lastName: string;
    emailAddress: string;
    streetLine1: string;
    postalCode: string;
    city: string;
}

const initialCustomer: CustomerForm = {
    firstName: 'Erika',
    lastName: 'Musterfrau',
    emailAddress: 'erika@example.test',
    streetLine1: 'Musterstraße 1',
    postalCode: '10115',
    city: 'Berlin',
};

function searchPrice(item: SearchItem): number {
    return item.priceWithTax.value ?? item.priceWithTax.min ?? 0;
}

async function shopApi<T>(query: string, variables?: Record<string, unknown>): Promise<T> {
    const response = await fetch('/api/shop', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ query, variables }),
    });
    const envelope = (await response.json()) as GraphQlEnvelope<T>;
    const error = graphQlError(envelope);
    if (!response.ok || error || !envelope.data) {
        throw new Error(error ?? `Shop API returned ${response.status}`);
    }
    return envelope.data;
}

const orderFields = `
    id code state totalWithTax currencyCode
    lines { id quantity linePriceWithTax productVariant { name sku } }
`;

function mutationError(value: unknown): string | undefined {
    if (value && typeof value === 'object' && 'errorCode' in value) {
        const error = value as { errorCode: string; message?: string };
        return error.message ?? error.errorCode;
    }
    return undefined;
}

export function Shop() {
    const [products, setProducts] = useState<SearchItem[]>([]);
    const [order, setOrder] = useState<Order>();
    const [customer, setCustomer] = useState(initialCustomer);
    const [busy, setBusy] = useState(false);
    const [notice, setNotice] = useState('');

    const load = useCallback(async () => {
        try {
            const data = await shopApi<{
                search: { items: SearchItem[] };
                activeOrder?: Order;
            }>(`query Storefront { search(input: { take: 50 }) { items { productVariantId productName productVariantName sku priceWithTax { __typename ... on SinglePrice { value } ... on PriceRange { min max } } currencyCode } } activeOrder { ${orderFields} } }`);
            setProducts(data.search.items);
            setOrder(data.activeOrder);
        } catch (error) {
            setNotice(String(error));
        }
    }, []);

    useEffect(() => void load(), [load]);

    async function add(productVariantId: string) {
        setBusy(true);
        setNotice('');
        try {
            const data = await shopApi<{ addItemToOrder: Order | { errorCode: string; message: string } }>(
                `mutation Add($id: ID!) { addItemToOrder(productVariantId: $id, quantity: 1) { ... on Order { ${orderFields} } ... on ErrorResult { errorCode message } } }`,
                { id: productVariantId },
            );
            const error = mutationError(data.addItemToOrder);
            if (error) throw new Error(error);
            setOrder(data.addItemToOrder as Order);
        } catch (error) {
            setNotice(String(error));
        } finally {
            setBusy(false);
        }
    }

    async function checkout(event: FormEvent) {
        event.preventDefault();
        setBusy(true);
        setNotice('');
        try {
            const customerResult = await shopApi<{ setCustomerForOrder: unknown }>(
                `mutation Customer($input: CreateCustomerInput!) { setCustomerForOrder(input: $input) { ... on Order { id } ... on ErrorResult { errorCode message } } }`,
                { input: { firstName: customer.firstName, lastName: customer.lastName, emailAddress: customer.emailAddress } },
            );
            const customerError = mutationError(customerResult.setCustomerForOrder);
            if (customerError) throw new Error(customerError);

            const addressResult = await shopApi<{ setOrderShippingAddress: unknown }>(
                `mutation Address($input: CreateAddressInput!) { setOrderShippingAddress(input: $input) { ... on Order { id } ... on ErrorResult { errorCode message } } }`,
                { input: { fullName: `${customer.firstName} ${customer.lastName}`, streetLine1: customer.streetLine1, postalCode: customer.postalCode, city: customer.city, countryCode: 'DE' } },
            );
            const addressError = mutationError(addressResult.setOrderShippingAddress);
            if (addressError) throw new Error(addressError);

            const shipping = await shopApi<{ eligibleShippingMethods: Array<{ id: string }> }>(
                `query Shipping { eligibleShippingMethods { id } }`,
            );
            if (!shipping.eligibleShippingMethods[0]) throw new Error('Keine Versandart verfügbar');
            const shippingResult = await shopApi<{ setOrderShippingMethod: unknown }>(
                `mutation ShippingMethod($id: [ID!]!) { setOrderShippingMethod(shippingMethodId: $id) { ... on Order { id } ... on ErrorResult { errorCode message } } }`,
                { id: [shipping.eligibleShippingMethods[0].id] },
            );
            const shippingError = mutationError(shippingResult.setOrderShippingMethod);
            if (shippingError) throw new Error(shippingError);

            const transition = await shopApi<{ transitionOrderToState: unknown }>(
                `mutation Arrange { transitionOrderToState(state: "ArrangingPayment") { ... on Order { id } ... on ErrorResult { errorCode message } } }`,
            );
            const transitionError = mutationError(transition.transitionOrderToState);
            if (transitionError) throw new Error(transitionError);

            const payments = await shopApi<{ eligiblePaymentMethods: Array<{ code: string }> }>(
                `query Payments { eligiblePaymentMethods { code } }`,
            );
            if (!payments.eligiblePaymentMethods[0]) throw new Error('Keine Testzahlung verfügbar');
            const paid = await shopApi<{ addPaymentToOrder: Order | { errorCode: string; message: string } }>(
                `mutation Pay($input: PaymentInput!) { addPaymentToOrder(input: $input) { ... on Order { ${orderFields} } ... on ErrorResult { errorCode message } } }`,
                { input: { method: payments.eligiblePaymentMethods[0].code, metadata: {} } },
            );
            const paymentError = mutationError(paid.addPaymentToOrder);
            if (paymentError) throw new Error(paymentError);
            setOrder(paid.addPaymentToOrder as Order);
            setNotice('Testbestellung bezahlt. Der Import in Essentials+ Merchant läuft über die Outbox.');
        } catch (error) {
            setNotice(String(error));
        } finally {
            setBusy(false);
        }
    }

    return (
        <main>
            <header>
                <span className="eyebrow">Essentials+ Merchant · Vendure 3.7</span>
                <h1>Ein kleiner, ehrlicher Testshop.</h1>
                <p>Artikel, Preise und Bestand kommen aus dem Core. Warenkorb und Testzahlung laufen ausschließlich über Vendure.</p>
            </header>

            {notice && <div className="notice">{notice}</div>}

            <section>
                <h2>Sortiment</h2>
                <div className="products">
                    {products.map(product => (
                        <article key={product.productVariantId}>
                            <span className="sku">{product.sku}</span>
                            <h3>{product.productName}</h3>
                            <strong>{formatMoney(searchPrice(product), product.currencyCode)}</strong>
                            <button disabled={busy} onClick={() => void add(product.productVariantId)}>In den Warenkorb</button>
                        </article>
                    ))}
                    {!products.length && <p>Noch keine Artikel projiziert.</p>}
                </div>
            </section>

            <section className="checkout">
                <div>
                    <h2>Warenkorb</h2>
                    {order?.lines.map(line => (
                        <p key={line.id}>{line.quantity} × {line.productVariant.name} <span>{formatMoney(line.linePriceWithTax, order.currencyCode)}</span></p>
                    ))}
                    <strong>{order ? formatMoney(order.totalWithTax, order.currencyCode) : 'Leer'}</strong>
                    {order && <small>Bestellung {order.code} · {order.state}</small>}
                </div>
                <form onSubmit={checkout}>
                    <h2>Testcheckout</h2>
                    {Object.entries(customer).map(([key, value]) => (
                        <label key={key}>{key === 'emailAddress' ? 'E-Mail' : key}
                            <input required type={key === 'emailAddress' ? 'email' : 'text'} value={value} onChange={event => setCustomer(current => ({ ...current, [key]: event.target.value }))} />
                        </label>
                    ))}
                    <button disabled={busy || !order?.lines.length} type="submit">Mit Testzahlung bestellen</button>
                </form>
            </section>
        </main>
    );
}
