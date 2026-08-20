#!/usr/bin/env node
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const transportPath = join(root, "backend/crates/server/src/marketplace.rs");
const transport = readFileSync(transportPath, "utf8");
const productionTransport = transport.split("#[cfg(test)]\nmod tests")[0];
const expectedVariants = [
  "LwaTokenRefresh",
  "CreateReport",
  "GetReport",
  "GetReportDocument",
  "DownloadReportDocument",
];

function fail(message) {
  process.stderr.write(`${message}\n`);
  process.exitCode = 1;
}

const enumBody = transport.match(/pub enum AmazonOperation\s*\{([\s\S]*?)\n\}/)?.[1] ?? "";
const enumVariants = [...enumBody.matchAll(/^\s*([A-Z][A-Za-z0-9]+),\s*$/gm)].map((match) => match[1]);
const allowlistBody = transport.match(/AMAZON_PILOT_OPERATION_ALLOWLIST[^=]*=\s*&\[([\s\S]*?)\];/)?.[1] ?? "";
const allowlistVariants = [...allowlistBody.matchAll(/AmazonOperation::([A-Za-z0-9]+)/g)].map((match) => match[1]);
if (JSON.stringify(enumVariants) !== JSON.stringify(expectedVariants)) {
  fail(`Amazon operation enum changed: ${JSON.stringify(enumVariants)}`);
}
if (JSON.stringify(allowlistVariants) !== JSON.stringify(expectedVariants)) {
  fail(`Amazon operation allowlist changed: ${JSON.stringify(allowlistVariants)}`);
}

const forbiddenApiMarkers = [
  "/listings/", "/products/pricing/", "/orders/", "/feeds/",
  "/fulfillment/", "/fba/", "/advertising/", "patchListingsItem",
  "putListingsItem", "deleteListingsItem", "updateInventory",
];
for (const marker of forbiddenApiMarkers) {
  if (productionTransport.toLowerCase().includes(marker.toLowerCase())) {
    fail(`Forbidden Amazon mutation API marker in production transport: ${marker}`);
  }
}
for (const method of ["put", "patch", "delete"]) {
  if (new RegExp(`\\.http\\.${method}\\s*\\(`).test(productionTransport)) {
    fail(`Forbidden direct Amazon HTTP ${method.toUpperCase()} builder`);
  }
}
const reviewedHttpCalls = [...productionTransport.matchAll(
  /\.http\s*\.\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(/g,
)].map((match) => match[1]);
if (JSON.stringify(reviewedHttpCalls) !== JSON.stringify(["post", "request", "get"])) {
  fail(`Amazon HTTP request construction changed outside the reviewed call sites: ${JSON.stringify(reviewedHttpCalls)}`);
}
if ([...productionTransport.matchAll(/reqwest::Client::builder\s*\(/g)].length !== 2
    || [...productionTransport.matchAll(/reqwest::RequestBuilder/g)].length !== 1
    || /reqwest::(?:get|Client::new|Client::default)/.test(productionTransport)) {
  fail("An unreviewed reqwest client or free request builder was added to the Amazon transport");
}

const sourceFiles = [];
function collect(path) {
  for (const entry of readdirSync(path)) {
    const full = join(path, entry);
    if (statSync(full).isDirectory()) collect(full);
    else if (full.endsWith(".rs") || full.endsWith("Cargo.toml")) sourceFiles.push(full);
  }
}
collect(join(root, "backend/crates"));

for (const file of sourceFiles) {
  const text = readFileSync(file, "utf8");
  const productionText = text.split("#[cfg(test)]\nmod tests")[0];
  const owner = relative(root, file);
  if (file !== transportPath && /sellingpartnerapi|x-amz-access-token/i.test(text)) {
    fail(`Amazon transport authority escaped its sole owner: ${owner}`);
  }
  if (file !== transportPath && /reqwest::(?:Client|RequestBuilder)|reqwest\s*=/.test(productionText)) {
    fail(`A second production HTTP client was introduced outside the reviewed Amazon transport: ${owner}`);
  }
  for (const marker of [
    "/listings/2021-08-01/items", "/products/pricing/", "/feeds/2021-06-30/feeds",
    "/fba/inventory/", "patchListingsItem", "putListingsItem", "deleteListingsItem",
    "createFeed", "updateInventory", "updatePricing", "advertising-api.amazon.com",
  ]) {
    if (productionText.toLowerCase().includes(marker.toLowerCase())) {
      fail(`Forbidden Amazon mutation marker outside the transport allowlist in ${owner}: ${marker}`);
    }
  }
  const providerSdk = /selling[-_]?partner[-_]?api|amazon[-_]?sp[-_]?api|aws-sdk-(?:marketplace|supplychain|advertising)/i;
  if ((file.endsWith("Cargo.toml") && providerSdk.test(text))
      || text.split("\n").some((line) => /^\s*(?:use|extern crate)\s/.test(line) && providerSdk.test(line))) {
    fail(`Unapproved Amazon provider SDK/import found: ${owner}`);
  }
}

const amazonHostLiterals = [...productionTransport.matchAll(/https:\/\/sellingpartnerapi-[a-z]+\.amazon\.com/g)].map((match) => match[0]);
if (JSON.stringify(amazonHostLiterals) !== JSON.stringify([
  "https://sellingpartnerapi-na.amazon.com",
  "https://sellingpartnerapi-eu.amazon.com",
  "https://sellingpartnerapi-fe.amazon.com",
])) {
  fail(`Amazon SP-API host boundary changed: ${JSON.stringify(amazonHostLiterals)}`);
}

if (!process.exitCode) {
  process.stdout.write(JSON.stringify({
    profile: "amazon-read-only",
    operations: expectedVariants,
    transport_owner: relative(root, transportPath),
    result: "passed",
  }) + "\n");
}
