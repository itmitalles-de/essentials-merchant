import assert from 'node:assert/strict';
import test from 'node:test';
import { formatMoney, graphQlError } from './shop-api';

test('formats Vendure minor currency units for German customers', () => {
    assert.match(formatMoney(1190), /11,90/);
});

test('combines Shop API errors without hiding them', () => {
    assert.equal(
        graphQlError({ errors: [{ message: 'first' }, { message: 'second' }] }),
        'first; second',
    );
});
