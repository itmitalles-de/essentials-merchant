import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const backupDir = process.argv[2];
if (!backupDir) throw new Error("pilot backup directory is required");
const manifest = JSON.parse(readFileSync(join(backupDir, "manifest.json"), "utf8"));
if (manifest.format !== "essentials-plus-merchant-amazon-pilot-backup-v1"
    || manifest.profile !== "amazon-read-only") {
  throw new Error("unsupported pilot backup format or profile");
}
for (const [name, expected] of Object.entries(manifest.files ?? {})) {
  const content = readFileSync(join(backupDir, name));
  const digest = createHash("sha256").update(content).digest("hex");
  if (digest !== expected.sha256 || content.byteLength !== expected.bytes) {
    throw new Error(`checksum or size mismatch: ${name}`);
  }
}
for (const store of [
  "core-schema", "pilot-core-data", "immutable-amazon-raw-archives",
  "normalized-amazon-snapshots", "deterministic-analysis-results", "module-states",
  "administrative-audit", "pilot-documents", "redacted-compose-metadata",
]) {
  if (!manifest.stores.includes(store)) throw new Error(`pilot manifest lacks store: ${store}`);
}
if (!/^[0-9a-f]{40}$/.test(manifest.repository_revision)
    || !manifest.parser_versions?.declared?.includes("sales-traffic-json-v2")
    || !Object.keys(manifest.container_images?.runtime_image_digests ?? {}).length) {
  throw new Error("pilot manifest lacks commit, parser, or image digest metadata");
}
for (const exclusion of ["LWA refresh tokens", "OAuth client secrets", "access tokens", "real buyer data"]) {
  if (!manifest.exclusions.includes(exclusion)) throw new Error(`pilot manifest lacks exclusion: ${exclusion}`);
}
process.stdout.write(`${JSON.stringify({ result: "verified", checksums: Object.keys(manifest.files).length, profile: manifest.profile })}\n`);
