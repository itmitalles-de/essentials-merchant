import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const backupDir = process.argv[2];
if (!backupDir) throw new Error('backup directory is required');
const manifest = JSON.parse(readFileSync(join(backupDir, 'manifest.json'), 'utf8'));
if (manifest.format !== 'essentials-plus-merchant-backup-v1') {
    throw new Error(`unsupported backup format: ${manifest.format}`);
}
for (const [name, expected] of Object.entries(manifest.files ?? {})) {
    const content = readFileSync(join(backupDir, name));
    const actual = createHash('sha256').update(content).digest('hex');
    if (actual !== expected.sha256 || content.byteLength !== expected.bytes) {
        throw new Error(`checksum or size mismatch: ${name}`);
    }
}
const requiredStores = [
    'core-postgres', 'vendure-postgres', 'core-documents', 'vendure-assets',
    'module-configurations-without-secrets', 'integration-mappings-inbox-outbox',
    'marketplace-raw-and-normalized-data', 'redacted-compose-metadata',
    'parser-versions', 'git-commit-and-image-digests',
];
for (const store of requiredStores) {
    if (!manifest.stores.includes(store)) throw new Error(`manifest lacks store: ${store}`);
}
if (!/^[0-9a-f]{40}$/.test(manifest.repository_revision)
    || !Object.keys(manifest.container_images?.runtime_image_digests ?? {}).length
    || !manifest.parser_versions?.declared?.includes('sales-traffic-json-v2')) {
    throw new Error('manifest lacks revision, runtime image digests, or parser versions');
}
process.stdout.write(`Verified ${Object.keys(manifest.files).length} backup checksums.\n`);
