import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import './styles.css';

export const metadata: Metadata = {
    title: 'Essentials+ Merchant – Testshop',
    description: 'German test storefront backed exclusively by the Vendure Shop API',
};

export default function RootLayout({ children }: Readonly<{ children: ReactNode }>) {
    return (
        <html lang="de">
            <body>{children}</body>
        </html>
    );
}
