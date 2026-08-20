#!/usr/bin/env node
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const profileIndex = process.argv.indexOf("--profile");
const outputIndex = process.argv.indexOf("--output");
const profile = profileIndex >= 0 ? process.argv[profileIndex + 1] : "pilot";
const output = outputIndex >= 0 ? resolve(process.argv[outputIndex + 1]) : "";
if (!output || !["pilot", "commerce"].includes(profile)) {
  process.stderr.write("Usage: scripts/generate-sbom.mjs --profile pilot|commerce --output PATH\n");
  process.exit(2);
}

const sha256 = (value) => createHash("sha256").update(value).digest("hex");
const componentKey = (component) => `${component.type}|${component.name}|${component.version}|${component["bom-ref"]}`;
const components = [];

function npmName(path, entry) {
  if (entry.name) return entry.name;
  const marker = "/node_modules/";
  const normalized = path.startsWith("node_modules/") ? `/${path}` : path;
  const tail = normalized.slice(normalized.lastIndexOf(marker) + marker.length);
  const parts = tail.split("/");
  return parts[0].startsWith("@") ? `${parts[0]}/${parts[1]}` : parts[0];
}

function addNpm(lockPath, scope, omitDev) {
  const raw = readFileSync(resolve(root, lockPath));
  const lock = JSON.parse(raw);
  for (const [path, entry] of Object.entries(lock.packages ?? {})) {
    if (!path.includes("node_modules/") || !entry.version || (omitDev && entry.dev)) continue;
    const name = npmName(path, entry);
    components.push({
      type: "library",
      name,
      version: entry.version,
      purl: `pkg:npm/${encodeURIComponent(name)}@${encodeURIComponent(entry.version)}`,
      "bom-ref": `npm:${scope}:${path}`,
      properties: [
        { name: "essentials:scope", value: scope },
        { name: "essentials:lock-path", value: path },
      ],
    });
  }
  return { path: lockPath, sha256: sha256(raw) };
}

function addCargo(lockPath) {
  const raw = readFileSync(resolve(root, lockPath), "utf8");
  for (const block of raw.split("[[package]]").slice(1)) {
    const name = block.match(/^\s*name\s*=\s*"([^"]+)"/m)?.[1];
    const version = block.match(/^\s*version\s*=\s*"([^"]+)"/m)?.[1];
    const source = block.match(/^\s*source\s*=\s*"([^"]+)"/m)?.[1] ?? "workspace";
    if (!name || !version) continue;
    components.push({
      type: "library",
      name,
      version,
      purl: `pkg:cargo/${encodeURIComponent(name)}@${encodeURIComponent(version)}`,
      "bom-ref": `cargo:${name}@${version}:${sha256(source).slice(0, 12)}`,
      properties: [{ name: "essentials:scope", value: "backend" }],
    });
  }
  return { path: lockPath, sha256: sha256(raw) };
}

const locks = [];
if (profile === "pilot") {
  locks.push(addNpm("frontend/package-lock.json", "frontend-runtime", true));
  locks.push(addCargo("backend/Cargo.lock"));
  for (const [name, version, digest] of [
    ["postgres", "16-alpine", "sha256:cf78e76683b9ca8c5733cbbdce6c9262b45b6767934dd0a95e671f9a0fc20685"],
    ["rust", "1.90-bookworm", "sha256:3914072ca0c3b8aad871db9169a651ccfce30cf58303e5d6f2db16d1d8a7e58f"],
    ["debian", "bookworm-slim", "sha256:abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241"],
    ["node", "22-alpine", "sha256:c610fcdfb1d5b4740dd70c284ed3cb16bb857e0f7166196e36a5501df7a3aa32"],
    ["nginx", "1.27-alpine", "sha256:65645c7bb6a0661892a8b03b89d0743208a18dd2f3f17a54ef4b76fb8e2f2a10"],
  ]) {
    components.push({
      type: "container",
      name,
      version,
      "bom-ref": `oci:${name}@${digest}`,
      properties: [
        { name: "essentials:scope", value: "pilot-container" },
        { name: "essentials:image-digest", value: digest },
      ],
    });
  }
  components.push(
    { type: "library", name: "sales-traffic-json-parser", version: "2", "bom-ref": "parser:sales-traffic-json-v2" },
  );
} else {
  locks.push(addNpm("commerce/package-lock.json", "commerce-production", true));
}

components.sort((left, right) => componentKey(left).localeCompare(componentKey(right)));
const serialSeed = sha256(`${profile}\n${locks.map((lock) => lock.sha256).join("\n")}\n${components.map(componentKey).join("\n")}`);
const serial = `${serialSeed.slice(0, 8)}-${serialSeed.slice(8, 12)}-4${serialSeed.slice(13, 16)}-a${serialSeed.slice(17, 20)}-${serialSeed.slice(20, 32)}`;
const buildRevision = /^[0-9a-f]{40}$/.test(process.env.GITHUB_SHA ?? "")
  ? process.env.GITHUB_SHA
  : null;
const bom = {
  bomFormat: "CycloneDX",
  specVersion: "1.5",
  serialNumber: `urn:uuid:${serial}`,
  version: 1,
  metadata: {
    component: {
      type: "application",
      name: profile === "pilot" ? "Essentials+ Merchant Amazon Intelligence Pilot" : "Essentials+ Merchant deferred Commerce",
      version: `lockset-${serialSeed.slice(0, 12)}`,
      "bom-ref": `application:${profile}:lockset:${serialSeed}`,
    },
    properties: [
      { name: "essentials:repository", value: "itmitalles-de/essentials-merchant" },
      { name: "essentials:generation", value: "deterministic lockfile-derived inventory; no runtime attestation" },
      ...(buildRevision ? [{ name: "essentials:build-commit", value: buildRevision }] : []),
      ...locks.flatMap((lock) => [
        { name: `essentials:lock:${lock.path}`, value: lock.sha256 },
      ]),
    ],
  },
  components,
};

mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, `${JSON.stringify(bom, null, 2)}\n`, { mode: 0o644 });
process.stdout.write(JSON.stringify({ profile, components: components.length, output, result: "passed" }) + "\n");
