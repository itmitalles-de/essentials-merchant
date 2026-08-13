export interface GraphQlEnvelope<T> {
    data?: T;
    errors?: Array<{ message: string }>;
}

export function graphQlError(response: GraphQlEnvelope<unknown>): string | undefined {
    return response.errors?.map(error => error.message).join('; ');
}

export function formatMoney(cents: number, currencyCode = 'EUR'): string {
    return new Intl.NumberFormat('de-DE', {
        style: 'currency',
        currency: currencyCode,
    }).format(cents / 100);
}
