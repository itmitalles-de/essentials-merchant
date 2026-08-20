import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";

const [backupDir, revision, schemaVersion] = process.argv.slice(2);
if (!backupDir || !/^[0-9a-f]{40}$/.test(revision ?? "")) {
  throw new Error("pilot backup manifest arguments are incomplete");
}

function filesBelow(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? filesBelow(path) : [path];
  });
}

const files = Object.fromEntries(filesBelow(join(backupDir, "data")).sort().map((path) => {
  const content = readFileSync(path);
  return [relative(backupDir, path), {
    bytes: content.byteLength,
    sha256: createHash("sha256").update(content).digest("hex"),
  }];
}));
const compose = JSON.parse(readFileSync(join(backupDir, "data/compose-metadata.json"), "utf8"));
const parserVersions = JSON.parse(readFileSync(join(backupDir, "data/parser-versions.json"), "utf8"));
const runtimeImages = Object.fromEntries(
  readFileSync(join(backupDir, "data/runtime-image-digests.tsv"), "utf8")
    .trim().split("\n").filter(Boolean).map((line) => line.split("\t")),
);

const manifest = {
  format: "essentials-plus-merchant-amazon-pilot-backup-v1",
  profile: "amazon-read-only",
  created_at: new Date().toISOString(),
  repository_revision: revision,
  schema_version: Number(schemaVersion),
  parser_versions: parserVersions,
  container_images: {
    declared: Object.fromEntries(Object.entries(compose.services ?? {})
      .map(([name, service]) => [name, service.image])),
    runtime_image_digests: runtimeImages,
  },
  stores: [
    "core-schema",
    "pilot-core-data",
    "immutable-amazon-raw-archives",
    "normalized-amazon-snapshots",
    "deterministic-analysis-results",
    "module-states",
    "administrative-audit",
    "pilot-documents",
    "redacted-compose-metadata",
  ],
  exclusions: [
    "LWA refresh tokens",
    "OAuth client secrets",
    "access tokens",
    "connection secret values",
    "Vendure and Storefront data",
    "Commerce customer, order, invoice, payment, and shipment data",
    "real buyer data",
  ],
  consistency: "The Rust backend and frontend were quiesced while PostgreSQL schema/data and pilot documents were captured.",
  files,
};
writeFileSync(join(backupDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`, { flag: "wx" });
process.stdout.write(`${JSON.stringify({ result: "created", files: Object.keys(files).length, profile: manifest.profile })}\n`);
