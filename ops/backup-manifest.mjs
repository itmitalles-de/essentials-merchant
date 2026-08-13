import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join, relative } from 'node:path';

const [backupDir, revision, coreSchema, vendureSchema] = process.argv.slice(2);
if (!backupDir || !revision) throw new Error('backup manifest arguments are incomplete');

function filesBelow(directory) {
    return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
        const path = join(directory, entry.name);
        return entry.isDirectory() ? filesBelow(path) : [path];
    });
}

const files = Object.fromEntries(
    filesBelow(join(backupDir, 'data')).sort().map(path => [
        relative(backupDir, path),
        {
            bytes: readFileSync(path).byteLength,
            sha256: createHash('sha256').update(readFileSync(path)).digest('hex'),
        },
    ]),
);
const manifest = {
    format: 'essentials-plus-merchant-backup-v1',
    created_at: new Date().toISOString(),
    repository_revision: revision,
    application_versions: {
        core: '0.1.0',
        commerce: '0.1.0',
        product: 'Essentials+ Merchant',
    },
    schema_versions: {
        core_sqlx: Number(coreSchema),
        vendure_typeorm: Number(vendureSchema),
    },
    stores: [
        'core-postgres',
        'vendure-postgres',
        'core-documents',
        'vendure-assets',
        'module-configurations-without-secrets',
        'integration-mappings-inbox-outbox',
        'marketplace-raw-and-normalized-data',
        'redacted-compose-metadata',
    ],
    consistency: 'Core and Vendure writers were quiesced before both logical dumps and volume archives.',
    files,
};
writeFileSync(join(backupDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx' });
process.stdout.write(`manifest ${basename(backupDir)}: ${Object.keys(files).length} checksums\n`);
