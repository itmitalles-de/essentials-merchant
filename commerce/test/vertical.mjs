import assert from 'node:assert/strict';

const coreApi = process.env.CORE_API_URL ?? 'http://127.0.0.1:8090/api';
const storefrontApi = process.env.STOREFRONT_API_URL ?? 'http://127.0.0.1:3001/api/shop';
const username = required('CORE_ADMIN_USERNAME');
const password = required('CORE_ADMIN_PASSWORD');
const suffix = `${Date.now()}-${Math.random().toString(16).slice(2, 8)}`;
const sku = `E2E-${suffix}`;
let storefrontCookie = '';

function required(name) {
    const value = process.env[name];
    if (!value) throw new Error(`${name} must be set for the vertical test`);
    return value;
}

async function jsonFetch(url, init = {}) {
    const response = await fetch(url, init);
    const text = await response.text();
    const body = text ? JSON.parse(text) : undefined;
    if (!response.ok) {
        throw new Error(`${init.method ?? 'GET'} ${url} returned ${response.status}: ${text}`);
    }
    return body;
}

async function core(path, token, init = {}) {
    return jsonFetch(new URL(path, `${coreApi}/`), {
        ...init,
        headers: {
            'content-type': 'application/json',
            authorization: `Bearer ${token}`,
            ...init.headers,
        },
    });
}

async function shop(query, variables) {
    const response = await fetch(storefrontApi, {
        method: 'POST',
        headers: {
            'content-type': 'application/json',
            ...(storefrontCookie ? { cookie: storefrontCookie } : {}),
        },
        body: JSON.stringify({ query, variables }),
    });
    const setCookie = response.headers.get('set-cookie');
    if (setCookie) storefrontCookie = setCookie.split(';', 1)[0];
    const body = await response.json();
    if (!response.ok || body.errors?.length) {
        throw new Error(`Storefront Shop API failed: ${JSON.stringify(body.errors ?? body)}`);
    }
    return body.data;
}

async function eventually(label, operation, timeoutMs = 45_000) {
    const deadline = Date.now() + timeoutMs;
    let lastError;
    while (Date.now() < deadline) {
        try {
            const value = await operation();
            if (value) return value;
        } catch (error) {
            lastError = error;
        }
        await new Promise(resolve => setTimeout(resolve, 500));
    }
    throw new Error(`${label} did not complete within ${timeoutMs} ms${lastError ? `: ${lastError}` : ''}`);
}

function expectResult(value) {
    if (value?.errorCode) throw new Error(`${value.errorCode}: ${value.message}`);
    return value;
}

const login = await jsonFetch(new URL('auth/login', `${coreApi}/`), {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ username, password }),
});
const token = login.access_token;

const article = await core('articles', token, {
    method: 'POST',
    body: JSON.stringify({
        sku,
        name: 'Vertikaler Testartikel',
        unit: 'Stück',
        sales_price_net: '10.00',
        default_vat_rate_code: 'STANDARD',
        purchase_price_net: null,
        min_stock_quantity: '0',
        active: true,
    }),
});
await core(`articles/${article.id}/stock-movements`, token, {
    method: 'POST',
    body: JSON.stringify({ movement_type: 'in', quantity: '10', note: 'Vertical commerce test' }),
});

const projected = await eventually('Core product projection', async () => {
    const data = await shop(`query Products($term: String!) {
        search(input: { term: $term, take: 10 }) {
            items {
                productVariantId productName sku currencyCode
                priceWithTax { __typename ... on SinglePrice { value } ... on PriceRange { min max } }
            }
        }
    }`, { term: sku });
    return data.search.items.find(item => item.sku === sku);
});
assert.equal(projected.priceWithTax.value ?? projected.priceWithTax.min, 1190);

const orderFields = `id code state totalWithTax currencyCode
    lines { id quantity linePriceWithTax productVariant { id name sku } }`;
let order = expectResult((await shop(`mutation Add($id: ID!) {
    addItemToOrder(productVariantId: $id, quantity: 2) {
        ... on Order { ${orderFields} }
        ... on ErrorResult { errorCode message }
    }
}`, { id: projected.productVariantId })).addItemToOrder);
assert.equal(order.lines.find(line => line.productVariant.sku === sku)?.quantity, 2);

expectResult((await shop(`mutation Customer($input: CreateCustomerInput!) {
    setCustomerForOrder(input: $input) {
        ... on Order { id }
        ... on ErrorResult { errorCode message }
    }
}`, { input: { firstName: 'Erika', lastName: 'Musterfrau', emailAddress: `e2e-${suffix}@example.test` } })).setCustomerForOrder);

expectResult((await shop(`mutation Address($input: CreateAddressInput!) {
    setOrderShippingAddress(input: $input) {
        ... on Order { id }
        ... on ErrorResult { errorCode message }
    }
}`, { input: { fullName: 'Erika Musterfrau', streetLine1: 'Musterstraße 1', postalCode: '10115', city: 'Berlin', countryCode: 'DE' } })).setOrderShippingAddress);

const shippingMethods = (await shop(`query { eligibleShippingMethods { id } }`)).eligibleShippingMethods;
assert.ok(shippingMethods[0], 'a German test shipping method must be eligible');
expectResult((await shop(`mutation Shipping($id: [ID!]!) {
    setOrderShippingMethod(shippingMethodId: $id) {
        ... on Order { id }
        ... on ErrorResult { errorCode message }
    }
}`, { id: [shippingMethods[0].id] })).setOrderShippingMethod);

expectResult((await shop(`mutation {
    transitionOrderToState(state: "ArrangingPayment") {
        ... on Order { id state }
        ... on ErrorResult { errorCode message }
    }
}`)).transitionOrderToState);

const paymentMethods = (await shop(`query { eligiblePaymentMethods { code isEligible } }`)).eligiblePaymentMethods;
const testPayment = paymentMethods.find(method => method.isEligible);
assert.ok(testPayment, 'the test payment method must be eligible');
order = expectResult((await shop(`mutation Pay($input: PaymentInput!) {
    addPaymentToOrder(input: $input) {
        ... on Order { ${orderFields} }
        ... on ErrorResult { errorCode message }
    }
}`, { input: { method: testPayment.code, metadata: {} } })).addPaymentToOrder);
assert.match(order.state, /PaymentSettled|PaymentAuthorized/);

const imported = await eventually('exactly-once Core order import', async () => {
    const orders = await core('sales-orders', token);
    const matching = orders.filter(candidate => candidate.external_order_id === order.id);
    assert.ok(matching.length <= 1, `external Vendure order was imported ${matching.length} times`);
    return matching[0];
});
// Wait through both Authorized and Settled events, then prove the second delivery stayed idempotent.
await new Promise(resolve => setTimeout(resolve, 2_000));
const matchingOrders = (await core('sales-orders', token)).filter(
    candidate => candidate.external_order_id === order.id,
);
assert.equal(matchingOrders.length, 1);
assert.equal(imported.stock_booked_at !== null, true);

const updatedArticle = await core(`articles/${article.id}`, token);
assert.equal(Number(updatedArticle.stock_quantity), 8);
const movements = await core(`articles/${article.id}/stock-movements`, token);
assert.equal(movements.filter(movement => movement.reference_type === 'sales_order').length, 1);

const trackingNumber = `TRACK-${suffix}`;
await core(`sales-orders/${imported.id}/fulfill`, token, {
    method: 'POST',
    body: JSON.stringify({ shipping_carrier: 'dhl', tracking_number: trackingNumber }),
});

await eventually('fulfillment projection', async () => {
    const result = await shop(`query Order($code: String!) {
        orderByCode(code: $code) { state fulfillments { state trackingCode method } }
    }`, { code: order.code });
    return result.orderByCode?.fulfillments?.some(
        fulfillment => fulfillment.trackingCode === trackingNumber && fulfillment.state === 'Shipped',
    );
});

console.log(`Vertical commerce test passed for ${sku}: one Core order, stock 10 -> 8, tracking projected.`);
