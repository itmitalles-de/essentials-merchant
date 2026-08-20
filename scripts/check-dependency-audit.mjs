#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = resolve(root, process.argv[2] ?? "artifacts/security/dependency-audit.json");
const expectedCommerceAdvisories = [
  "GHSA-9q82-xgwf-vj6h", "GHSA-5v7r-6r5c-r473", "GHSA-w3rx-r6r6-pgpr",
  "GHSA-5p2g-fcmc-qvqq", "GHSA-r5fr-rjxr-66jc", "GHSA-f23m-r3pf-42rh",
  "GHSA-xxjr-mmjv-4gpg", "GHSA-f88m-g3jw-g9cj", "GHSA-w5hq-g745-h8pq",
  "GHSA-58qx-3vcg-4xpx", "GHSA-96hv-2xvq-fx4p",
].map((value) => value.toUpperCase()).sort();

function audit(directory) {
  const result = spawnSync("npm", ["audit", "--omit=dev", "--json"], {
    cwd: resolve(root, directory),
    encoding: "utf8",
    maxBuffer: 20 * 1024 * 1024,
  });
  if (!result.stdout.trim()) throw new Error(`npm audit produced no JSON for ${directory}`);
  return JSON.parse(result.stdout);
}

function advisoryIds(report) {
  const values = new Set();
  for (const vulnerability of Object.values(report.vulnerabilities ?? {})) {
    for (const via of vulnerability.via ?? []) {
      if (typeof via !== "object" || typeof via.url !== "string") continue;
      const id = via.url.match(/GHSA-[a-z0-9-]+/i)?.[0]?.toUpperCase();
      if (id) values.add(id);
    }
  }
  return [...values].sort();
}

const frontend = audit("frontend");
const commerce = audit("commerce");
const actualCommerceAdvisories = advisoryIds(commerce);
const summary = {
  generated_at: new Date().toISOString(),
  redaction: "summary and advisory identifiers only",
  frontend: frontend.metadata,
  commerce: {
    metadata: commerce.metadata,
    distinct_advisories: actualCommerceAdvisories,
    status: "open-triaged-not-remediated",
  },
};
mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, `${JSON.stringify(summary, null, 2)}\n`);

const frontendCount = frontend.metadata?.vulnerabilities?.total ?? -1;
const critical = commerce.metadata?.vulnerabilities?.critical ?? -1;
if (frontendCount !== 0 || critical !== 0
    || JSON.stringify(actualCommerceAdvisories) !== JSON.stringify(expectedCommerceAdvisories)) {
  process.stderr.write(`${JSON.stringify({ result: "failed", frontendCount, critical, actualCommerceAdvisories })}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ result: "passed", frontendCount, critical, commerceAdvisories: actualCommerceAdvisories.length })}\n`);
