import assert from 'node:assert/strict';
import { createHash, createHmac, randomUUID } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const repositoryRoot = fileURLToPath(new URL('../..', import.meta.url));
const project = required('COMPOSE_PROJECT_NAME');
const coreApi = process.env.CORE_API_URL ?? 'http://127.0.0.1:8090/api';
const storefrontApi = process.env.STOREFRONT_API_URL ?? 'http://127.0.0.1:3001/api/shop';
const suffix = `${Date.now()}-${randomUUID().slice(0, 8)}`;
let storefrontCookie = '';
let checkoutSequence = 0;

function required(name) {
    const value = process.env[name];
    if (!value) throw new Error(`${name} must be set for the recovery test`);
    return value;
}

function compose(args, timeout = 180_000) {
    try {
        return execFileSync(
            'docker',
            ['compose', '--env-file', '/dev/null', '-p', project, ...args],
            { cwd: repositoryRoot, encoding: 'utf8', timeout },
        ).trim();
    } catch (error) {
        throw new Error(
            `docker compose ${args.join(' ')} failed: ${error.stdout ?? ''}${error.stderr ?? ''}`,
        );
    }
}

function coreSql(query) {
    return compose(['exec', '-T', 'db', 'psql', '-U', 'erplite', '-d', 'erplite', '-qAt', '-v', 'ON_ERROR_STOP=1', '-c', query]);
}

function vendureSql(query) {
    const user = process.env.VENDURE_DB_USERNAME ?? 'vendure';
    const database = process.env.VENDURE_DB_NAME ?? 'vendure';
    return compose(['exec', '-T', 'vendure-db', 'psql', '-U', user, '-d', database, '-qAt', '-v', 'ON_ERROR_STOP=1', '-c', query]);
}

async function eventually(label, operation, timeoutMs = 60_000, intervalMs = 200) {
    const deadline = Date.now() + timeoutMs;
    let lastError;
    while (Date.now() < deadline) {
        try {
            const result = await operation();
            if (result) return result;
        } catch (error) {
            lastError = error;
        }
        await new Promise(resolve => setTimeout(resolve, intervalMs));
    }
    throw new Error(
        `${label} exceeded ${timeoutMs} ms${lastError ? `: ${String(lastError)}` : ''}`,
    );
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

async function waitForCore() {
    return eventually('Core readiness', async () => {
        const response = await fetch(new URL('readiness', `${coreApi}/`));
        return response.ok;
    });
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

function expectResult(value) {
    if (value?.errorCode) throw new Error(`${value.errorCode}: ${value.message}`);
    return value;
}

async function createArticle(token, sku, name) {
    const article = await core('articles', token, {
        method: 'POST',
        body: JSON.stringify({
            sku,
            name,
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
        body: JSON.stringify({
            movement_type: 'in',
            quantity: '10',
            note: 'Synthetic recovery test',
        }),
    });
    return article;
}

async function findProduct(sku) {
    const data = await shop(`query Products($term: String!) {
        search(input: { term: $term, take: 10 }) {
            items {
                productVariantId productName sku
                priceWithTax { __typename ... on SinglePrice { value } ... on PriceRange { min max } }
            }
        }
    }`, { term: sku });
    return data.search.items.find(item => item.sku === sku);
}

async function checkout(product, sku, expectPaymentFailure = false) {
    storefrontCookie = '';
    checkoutSequence += 1;
    const fields = `id code state totalWithTax currencyCode
        lines { id quantity linePriceWithTax productVariant { id name sku } }`;
    let order = expectResult((await shop(`mutation Add($id: ID!) {
        addItemToOrder(productVariantId: $id, quantity: 2) {
            ... on Order { ${fields} }
            ... on ErrorResult { errorCode message }
        }
    }`, { id: product.productVariantId })).addItemToOrder);
    assert.equal(order.lines.find(line => line.productVariant.sku === sku)?.quantity, 2);
    expectResult((await shop(`mutation Customer($input: CreateCustomerInput!) {
        setCustomerForOrder(input: $input) {
            ... on Order { id } ... on ErrorResult { errorCode message }
        }
    }`, { input: {
        firstName: 'Synthetic', lastName: 'Customer',
        emailAddress: `recovery-${suffix}-${checkoutSequence}@example.test`,
    } })).setCustomerForOrder);
    expectResult((await shop(`mutation Address($input: CreateAddressInput!) {
        setOrderShippingAddress(input: $input) {
            ... on Order { id } ... on ErrorResult { errorCode message }
        }
    }`, { input: {
        fullName: 'Synthetic Customer', streetLine1: 'Fixture Street 1',
        postalCode: '10115', city: 'Berlin', countryCode: 'DE',
    } })).setOrderShippingAddress);
    const methods = (await shop('query { eligibleShippingMethods { id } }')).eligibleShippingMethods;
    expectResult((await shop(`mutation Shipping($id: [ID!]!) {
        setOrderShippingMethod(shippingMethodId: $id) {
            ... on Order { id } ... on ErrorResult { errorCode message }
        }
    }`, { id: [methods[0].id] })).setOrderShippingMethod);
    expectResult((await shop(`mutation {
        transitionOrderToState(state: "ArrangingPayment") {
            ... on Order { id } ... on ErrorResult { errorCode message }
        }
    }`)).transitionOrderToState);
    const payment = (await shop('query { eligiblePaymentMethods { code isEligible } }'))
        .eligiblePaymentMethods.find(method => method.isEligible);
    const paymentResult = (await shop(`mutation Pay($input: PaymentInput!) {
        addPaymentToOrder(input: $input) {
            ... on Order { ${fields} } ... on ErrorResult { errorCode message }
        }
    }`, { input: { method: payment.code, metadata: {} } })).addPaymentToOrder;
    if (expectPaymentFailure) {
        assert.ok(paymentResult.errorCode, 'disabled payment module must reject direct payment');
        return undefined;
    }
    order = expectResult(paymentResult);
    return order;
}

async function setModule(token, moduleId, enabled, purpose) {
    return core(`modules/${moduleId}`, token, {
        method: 'PUT',
        headers: { 'idempotency-key': `${purpose}-${suffix}` },
        body: JSON.stringify({ enabled }),
    });
}

function signedHeaders(keyId, secret, method, path, body, timestamp, nonce) {
    const bodyHash = createHash('sha256').update(body).digest('hex');
    const signature = createHmac('sha256', secret)
        .update(`${method}\n${path}\n${timestamp}\n${nonce}\n${bodyHash}`)
        .digest('hex');
    return {
        'content-type': 'application/json',
        'x-essentials-key-id': keyId,
        'x-essentials-timestamp': String(timestamp),
        'x-essentials-nonce': nonce,
        'x-essentials-signature': signature,
    };
}

async function verifyRequestAuthentication() {
    const path = '/api/integrations/vendure/commands/claim';
    const body = '{"limit":1}';
    const timestamp = Math.floor(Date.now() / 1_000);
    const oldId = required('INTEGRATION_PREVIOUS_KEY_ID');
    const oldSecret = required('INTEGRATION_PREVIOUS_SECRET');
    const headers = signedHeaders(
        oldId, oldSecret, 'POST', path, body, timestamp, `old-${randomUUID()}`,
    );
    const first = await eventually('previous rotation key activation', async () => {
        const response = await fetch(new URL(path, coreApi), { method: 'POST', headers, body });
        return response.status === 200 ? response : undefined;
    }, 30_000);
    assert.equal(first.status, 200, 'previous rotation key must remain accepted');
    const replay = await fetch(new URL(path, coreApi), { method: 'POST', headers, body });
    assert.equal(replay.status, 409, 'the same nonce must be rejected');
    const invalid = await fetch(new URL(path, coreApi), {
        method: 'POST',
        headers: {
            ...headers,
            'x-essentials-nonce': `invalid-${randomUUID()}`,
            'x-essentials-signature': '00'.repeat(32),
        },
        body,
    });
    assert.equal(invalid.status, 401, 'invalid HMAC must be rejected');
    const expiredAt = timestamp - 600;
    const expired = await fetch(new URL(path, coreApi), {
        method: 'POST',
        headers: signedHeaders(
            'current', required('INTEGRATION_SECRET'), 'POST', path, body,
            expiredAt, `expired-${randomUUID()}`,
        ),
        body,
    });
    assert.equal(expired.status, 401, 'expired HMAC must be rejected');
}

// Recreating only processes clears one-shot failpoint markers without touching data stores.
compose(['up', '-d', '--force-recreate', '--no-deps', 'backend', 'vendure-worker']);
await waitForCore();
await verifyRequestAuthentication();

const login = await jsonFetch(new URL('auth/login', `${coreApi}/`), {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
        username: required('CORE_ADMIN_USERNAME'),
        password: required('CORE_ADMIN_PASSWORD'),
    }),
});
const token = login.access_token;
const catalog = await core('modules', token);
assert.ok(catalog.some(module => module.module_id === 'core.catalog' && module.required));
assert.ok(catalog.some(module => module.module_id === 'marketplace.amazon_intelligence' && !module.enabled));
const disabledMarketplace = await fetch(new URL('marketplace', `${coreApi}/`), {
    headers: { authorization: `Bearer ${token}` },
});
assert.equal(disabledMarketplace.status, 409, 'disabled module API must reject direct calls');
const disabledDatev = await fetch(new URL('exports/datev', `${coreApi}/`), {
    method: 'POST',
    headers: {
        authorization: `Bearer ${token}`,
        'content-type': 'application/json',
        'idempotency-key': `datev-disabled-${suffix}`,
    },
    body: '{}',
});
assert.equal(disabledDatev.status, 409, 'disabled DATEV module API must reject direct calls');
const invoiceCountBefore = Number(coreSql('SELECT count(*) FROM invoices'));

// Core unavailable before projection; worker crashes after claim and before acknowledge.
compose(['stop', 'vendure-worker']);
const primarySku = `RECOVERY-${suffix}`;
const primary = await createArticle(token, primarySku, 'Synthetic recovery product');
compose(['stop', 'backend']);
compose(['start', 'vendure-worker']);
assert.ok(Number(coreSql(`SELECT count(*) FROM integration_outbox WHERE aggregate_id = '${primary.id}' AND status = 'pending'`)) >= 1);
compose(['start', 'backend']);
await waitForCore();
await eventually('persisted worker claim', async () => {
    const count = Number(coreSql(`SELECT count(*) FROM integration_outbox WHERE aggregate_id = '${primary.id}' AND status = 'processing'`));
    return count > 0;
}, 90_000, 100);
const primaryProduct = await eventually('projection after service restarts', async () => {
    const product = await findProduct(primarySku);
    return (product?.priceWithTax.value ?? product?.priceWithTax.min) === 1190
        ? product
        : undefined;
});
await eventually('all initial Core events acknowledged', async () => {
    return Number(coreSql(`SELECT count(*) FROM integration_outbox WHERE aggregate_id = '${primary.id}' AND status != 'delivered'`)) === 0;
});
assert.ok(Number(coreSql(`SELECT max(attempts) FROM integration_outbox WHERE aggregate_id = '${primary.id}'`)) >= 3);

// Direct Vendure payment calls fail closed while the synthetic connector module is disabled.
await setModule(token, 'payment.test', false, 'disable-payment');
await checkout(primaryProduct, primarySku, true);
await setModule(token, 'payment.test', true, 'enable-payment');

// Payment import crosses worker-after-claim plus Core-before/after-inbox-commit crashes.
const order = await checkout(primaryProduct, primarySku);
const vendureOrderId = Number(order.id);
assert.ok(Number.isSafeInteger(vendureOrderId), 'Vendure order ID must be numeric in the test schema');
const imported = await eventually('exactly-once order import', async () => {
    const matching = (await core('sales-orders', token))
        .filter(candidate => candidate.external_order_id === order.id);
    assert.ok(matching.length <= 1, `external order imported ${matching.length} times`);
    return matching[0];
}, 90_000);
await eventually('Vendure outbox delivered after retries', async () => {
    const row = vendureSql(`SELECT min("status") || '|' || max("attempts") FROM shop_suite_integration_outbox WHERE payload->>'order_id' = '${order.id}'`);
    const [status, attempts] = row.split('|');
    return status === 'delivered' && Number(attempts) >= 4;
}, 90_000);
assert.equal((await core('sales-orders', token)).filter(item => item.external_order_id === order.id).length, 1);
assert.equal(Number((await core(`articles/${primary.id}`, token)).stock_quantity), 8);
assert.equal((await core(`articles/${primary.id}/stock-movements`, token)).filter(item => item.reference_type === 'sales_order').length, 1);
assert.equal(Number(vendureSql(`SELECT count(*) FROM payment WHERE "orderId" = ${vendureOrderId}`)), 1);
assert.equal(Number(coreSql('SELECT count(*) FROM invoices')), invoiceCountBefore);

// Vendure and its DB are unavailable while product and fulfillment events wait.
compose(['stop', 'vendure-worker']);
const secondarySku = `RECOVERY-DB-${suffix}`;
const secondary = await createArticle(token, secondarySku, 'Synthetic DB recovery product');
const tracking = `RECOVERY-TRACK-${suffix}`;
await setModule(token, 'shipping.manual', false, 'disable-shipping');
const disabledFulfillment = await fetch(
    new URL(`sales-orders/${imported.id}/fulfill`, `${coreApi}/`),
    {
        method: 'POST',
        headers: {
            authorization: `Bearer ${token}`,
            'content-type': 'application/json',
        },
        body: JSON.stringify({ shipping_carrier: 'dhl', tracking_number: tracking }),
    },
);
assert.equal(disabledFulfillment.status, 409, 'disabled shipping module must reject direct calls');
await setModule(token, 'shipping.manual', true, 'enable-shipping');
await core(`sales-orders/${imported.id}/fulfill`, token, {
    method: 'POST',
    body: JSON.stringify({ shipping_carrier: 'dhl', tracking_number: tracking }),
});
compose(['stop', 'vendure-db']);
assert.ok(Number(coreSql(`SELECT count(*) FROM integration_outbox WHERE aggregate_id = '${secondary.id}' AND status = 'pending'`)) >= 1);
compose(['start', 'vendure-db']);
compose(['up', '-d', '--wait', 'vendure-server', 'vendure-worker', 'storefront'], 180_000);
await eventually('product after Vendure DB restart', () => findProduct(secondarySku), 90_000);
await eventually('fulfillment after Vendure DB restart', async () => {
    const result = await shop(`query Order($code: String!) {
        orderByCode(code: $code) { fulfillments { state trackingCode } }
    }`, { code: order.code });
    return result.orderByCode?.fulfillments?.some(
        item => item.trackingCode === tracking && item.state === 'Shipped',
    );
}, 90_000);

// A delayed older projection cannot overwrite a newer one.
compose(['stop', 'vendure-worker']);
const state = await core(`articles/${primary.id}`, token);
for (const price of ['20.00', '30.00']) {
    await core(`articles/${primary.id}`, token, {
        method: 'PUT',
        body: JSON.stringify({
            sku: state.sku,
            name: state.name,
            unit: state.unit,
            sales_price_net: price,
            default_vat_rate_code: state.default_vat_rate_code,
            purchase_price_net: state.purchase_price_net,
            min_stock_quantity: state.min_stock_quantity,
            active: state.active,
        }),
    });
}
const rows = coreSql(`SELECT id || '|' || sequence FROM integration_outbox WHERE aggregate_id = '${primary.id}' AND status = 'pending' ORDER BY sequence DESC LIMIT 2`).split('\n');
assert.equal(rows.length, 2);
const [newerId] = rows[0].split('|');
const [olderId] = rows[1].split('|');
coreSql(`UPDATE integration_outbox SET available_at = CASE WHEN id = '${olderId}' THEN now() + interval '2 seconds' ELSE now() END WHERE id IN ('${olderId}', '${newerId}')`);
compose(['start', 'vendure-worker']);
await eventually('newer projection applied', async () => coreSql(`SELECT status FROM integration_outbox WHERE id = '${newerId}'`) === 'delivered');
await eventually('older delayed projection ignored', async () => coreSql(`SELECT status FROM integration_outbox WHERE id = '${olderId}'`) === 'delivered');
await eventually('newest price remains authoritative', async () => {
    const product = await findProduct(primarySku);
    return (product?.priceWithTax.value ?? product?.priceWithTax.min) === 3570;
});
assert.equal(Number((await core(`articles/${primary.id}`, token)).stock_quantity), 8);

// Repeated failures reach dead state; manual requeue is idempotent and audited.
const deadId = coreSql(`INSERT INTO integration_outbox (id, event_type, aggregate_type, aggregate_id, idempotency_key, payload) VALUES (gen_random_uuid(), 'vendure.unsupported.synthetic', 'article', '${primary.id}', 'recovery-dead:${suffix}', '{}'::jsonb) RETURNING id`);
await eventually('dead-letter state', async () => coreSql(`SELECT status FROM integration_outbox WHERE id = '${deadId}'`) === 'dead', 30_000);
compose(['stop', 'vendure-worker']);
const requeueKey = `requeue-${suffix}`;
const requeuePath = `integration-diagnostics/events/core/${deadId}/requeue`;
const first = await core(requeuePath, token, { method: 'POST', headers: { 'idempotency-key': requeueKey } });
const duplicate = await core(requeuePath, token, { method: 'POST', headers: { 'idempotency-key': requeueKey } });
assert.equal(first.accepted, true);
assert.equal(duplicate.duplicate, true);
assert.equal(coreSql(`SELECT requeue_count FROM integration_outbox WHERE id = '${deadId}'`), '1');
assert.equal(coreSql(`SELECT count(*) FROM administrative_audit_log WHERE idempotency_key = '${requeueKey}'`), '1');
compose(['start', 'vendure-worker']);
await eventually('requeued invalid event returns to dead', async () => coreSql(`SELECT status FROM integration_outbox WHERE id = '${deadId}'`) === 'dead', 30_000);

// Database and full-stack restarts retain the proven invariants.
compose(['restart', 'db', 'vendure-db']);
compose(['up', '-d', '--wait'], 180_000);
await waitForCore();
const fullRestartStartedAt = Date.now();
compose(['restart'], 120_000);
await waitForCore();
await eventually('Storefront and Vendure after full restart', async () => {
    try {
        const result = await shop('{ __typename }');
        return result.__typename === 'Query';
    } catch {
        return false;
    }
}, 120_000);
await eventually('worker diagnostics after full restart', async () => {
    const snapshot = await core('integration-diagnostics', token);
    return snapshot.vendure_observed_at &&
        Date.parse(snapshot.vendure_observed_at) >= fullRestartStartedAt;
}, 120_000);
assert.equal((await core('sales-orders', token)).filter(item => item.external_order_id === order.id).length, 1);
assert.equal(Number((await core(`articles/${primary.id}`, token)).stock_quantity), 8);
assert.equal((await core(`articles/${primary.id}/stock-movements`, token)).filter(item => item.reference_type === 'sales_order').length, 1);
assert.equal(Number(vendureSql(`SELECT count(*) FROM payment WHERE "orderId" = ${vendureOrderId}`)), 1);
assert.equal(Number(coreSql('SELECT count(*) FROM invoices')), invoiceCountBefore);
const diagnostics = await core('integration-diagnostics', token);
assert.ok(diagnostics.events.every(event => !('payload' in event)));
assert.ok(diagnostics.events.some(event => event.event_id === deadId && event.status === 'dead' && event.last_error));
assert.ok(diagnostics.audit.some(entry => entry.idempotency_key === requeueKey));
assert.ok(diagnostics.mappings.some(mapping => mapping.entity_type === 'article'));

console.log(
    `Recovery matrix passed for ${suffix}: restart-safe leases, exactly-once import, ` +
    'stale-event protection, dead-letter audit, HMAC rotation and persisted recovery verified.',
);
