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
const composeMetadata = JSON.parse(readFileSync(join(backupDir, 'data/compose-metadata.json'), 'utf8'));
const parserVersions = JSON.parse(readFileSync(join(backupDir, 'data/parser-versions.json'), 'utf8'));
const runtimeImageDigests = Object.fromEntries(
    readFileSync(join(backupDir, 'data/runtime-image-digests.tsv'), 'utf8')
        .trim().split('\n').filter(Boolean).map(line => line.split('\t')),
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
    parser_versions: parserVersions,
    container_images: {
        declared: Object.fromEntries(Object.entries(composeMetadata.services ?? {})
            .map(([name, service]) => [name, service.image])),
        runtime_image_digests: runtimeImageDigests,
    },
    stores: [
        'core-postgres',
        'vendure-postgres',
        'core-documents',
        'vendure-assets',
        'module-configurations-without-secrets',
        'integration-mappings-inbox-outbox',
        'marketplace-raw-and-normalized-data',
        'parser-versions',
        'git-commit-and-image-digests',
        'redacted-compose-metadata',
    ],
    consistency: 'Core and Vendure writers were quiesced before both logical dumps and volume archives.',
    files,
};
writeFileSync(join(backupDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx' });
process.stdout.write(`manifest ${basename(backupDir)}: ${Object.keys(files).length} checksums\n`);
