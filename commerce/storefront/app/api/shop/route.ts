import { cookies } from 'next/headers';
import { NextRequest, NextResponse } from 'next/server';

const sessionCookie = 'shop-suite-vendure-token';

export async function POST(request: NextRequest): Promise<NextResponse> {
    const endpoint = process.env.VENDURE_SHOP_API_URL;
    if (!endpoint) {
        return NextResponse.json({ errors: [{ message: 'Shop API is not configured' }] }, { status: 503 });
    }

    const body = await request.text();
    const cookieStore = await cookies();
    const token = cookieStore.get(sessionCookie)?.value;
    const headers: Record<string, string> = { 'content-type': 'application/json' };
    if (token) {
        headers.authorization = `Bearer ${token}`;
    }
    if (process.env.VENDURE_CHANNEL_TOKEN) {
        headers['vendure-token'] = process.env.VENDURE_CHANNEL_TOKEN;
    }

    try {
        const upstream = await fetch(endpoint, { method: 'POST', headers, body, cache: 'no-store' });
        const response = new NextResponse(await upstream.text(), {
            status: upstream.status,
            headers: { 'content-type': upstream.headers.get('content-type') ?? 'application/json' },
        });
        const nextToken = upstream.headers.get('vendure-auth-token');
        if (nextToken) {
            response.cookies.set(sessionCookie, nextToken, {
                httpOnly: true,
                sameSite: 'lax',
                secure: process.env.STOREFRONT_COOKIE_SECURE === 'true',
                path: '/',
            });
        }
        return response;
    } catch {
        return NextResponse.json({ errors: [{ message: 'Shop API is temporarily unavailable' }] }, { status: 503 });
    }
}
