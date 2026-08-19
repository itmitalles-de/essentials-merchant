#!/usr/bin/env node
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";

const args = Object.fromEntries(process.argv.slice(2).reduce((pairs, value, index, values) => {
  if (value.startsWith("--")) pairs.push([value.slice(2), values[index + 1]]);
  return pairs;
}, []));
const mode = args.mode ?? "check";
if (!["check", "execute"].includes(mode) || !args["env-file"] || !args["gate-file"] || !args.output) {
  throw new Error("staging gate arguments are incomplete");
}

function dotenv(path) {
  const values = {};
  for (const line of readFileSync(path, "utf8").split(/\r?\n/)) {
    if (!line.trim() || line.trimStart().startsWith("#")) continue;
    const separator = line.indexOf("=");
    if (separator < 1) continue;
    const key = line.slice(0, separator).trim();
    let value = line.slice(separator + 1).trim();
    if ((value.startsWith("'") && value.endsWith("'"))
        || (value.startsWith('"') && value.endsWith('"'))) value = value.slice(1, -1);
    values[key] = value;
  }
  return values;
}

const env = { ...dotenv(args["env-file"]), ...process.env };
const gate = JSON.parse(readFileSync(args["gate-file"], "utf8"));
const requiredStrings = ["seller_id", "region", "marketplace_id", "data_start_time", "data_end_time", "approval_reference"];
if (requiredStrings.some((field) => typeof gate[field] !== "string" || !gate[field].trim())) {
  throw new Error("staging gate lacks a required non-empty field");
}
if (!gate.seller_approved || !gate.marketplace_participation_confirmed
    || !gate.encrypted_archive_target_confirmed
    || !Array.isArray(gate.roles_confirmed) || !gate.roles_confirmed.includes("Brand Analytics")) {
  throw new Error("seller, marketplace, role, and encrypted archive approvals must all be explicit");
}
if (!new Set(["na", "eu", "fe"]).has(gate.region)) throw new Error("unsupported Amazon region");
const start = new Date(gate.data_start_time);
const end = new Date(gate.data_end_time);
const now = new Date();
const todayUtc = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
const periodDays = Math.floor((Date.UTC(end.getUTCFullYear(), end.getUTCMonth(), end.getUTCDate())
  - Date.UTC(start.getUTCFullYear(), start.getUTCMonth(), start.getUTCDate())) / 86_400_000) + 1;
if (!Number.isFinite(start.valueOf()) || !Number.isFinite(end.valueOf())
    || start >= end || end.valueOf() >= todayUtc || periodDays < 1 || periodDays > 7) {
  throw new Error("staging period must be a completed UTC period of one to seven calendar days");
}

let secret;
let approval;
try {
  secret = JSON.parse(env.AMAZON_SECRET_PILOT_SELLER ?? "");
  approval = JSON.parse(env.AMAZON_STAGING_APPROVAL ?? "");
} catch {
  throw new Error("pilot secret reference or staging approval has invalid JSON");
}
if (!["refresh_token", "client_id", "client_secret"].every((field) =>
  typeof secret[field] === "string" && secret[field].trim())) {
  throw new Error("pilot secret reference is incomplete");
}
const sellerHash = createHash("sha256").update(gate.seller_id).digest("hex");
if (approval.seller_sha256 !== sellerHash || approval.region !== gate.region
    || approval.marketplace_id !== gate.marketplace_id) {
  throw new Error("staging approval does not match the gated seller context");
}
if (!env.ADMIN_USERNAME || !env.ADMIN_PASSWORD) throw new Error("pilot administrator login is not configured");

const baseUrl = `http://127.0.0.1:${env.PILOT_FRONTEND_PORT || "8090"}/api`;
async function request(path, options = {}, token) {
  const response = await fetch(`${baseUrl}${path}`, {
    ...options,
    headers: {
      ...(options.body ? { "content-type": "application/json" } : {}),
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
  });
  if (!response.ok) throw new Error(`pilot API rejected ${path} with HTTP ${response.status}`);
  return response.json();
}

let token;
try {
  token = (await request("/auth/login", {
    method: "POST",
    body: JSON.stringify({ username: env.ADMIN_USERNAME, password: env.ADMIN_PASSWORD }),
  })).access_token;
} catch (error) {
  if (mode === "check") {
    process.stdout.write(`${JSON.stringify({ result: "blocked", reason: "pilot-runtime-unavailable-or-login-rejected" })}\n`);
    process.exit(3);
  }
  throw error;
}
const pilot = await request("/pilot/status", {}, token);
if (!pilot.enabled || !pilot.compliant || pilot.automatic_schedules_enabled !== 0) {
  throw new Error("persisted Amazon pilot profile is not compliant");
}
if (mode === "check") {
  process.stdout.write(`${JSON.stringify({
    result: "ready",
    report_type: "GET_SALES_AND_TRAFFIC_REPORT",
    period_days: periodDays,
    seller: `sha256:${sellerHash.slice(0, 12)}`,
    credentials: "shape-valid",
    secrets_printed: false,
  })}\n`);
  process.exit(0);
}

const connection = await request("/marketplace/connections", {
  method: "POST",
  body: JSON.stringify({
    seller_id: gate.seller_id,
    region: gate.region,
    secret_ref: "pilot_seller",
    granted_roles: ["Brand Analytics"],
    marketplace_ids: [gate.marketplace_id],
    mode: "live",
    enabled: true,
  }),
}, token);
const startedAt = new Date();
const run = await request(`/marketplace/connections/${connection.id}/runs`, {
  method: "POST",
  body: JSON.stringify({
    marketplace_id: gate.marketplace_id,
    report_type: "GET_SALES_AND_TRAFFIC_REPORT",
    data_start_time: start.toISOString(),
    data_end_time: end.toISOString(),
    report_options: { dateGranularity: "DAY", asinGranularity: "CHILD" },
  }),
}, token);

let detail;
for (let attempt = 0; attempt < 180; attempt += 1) {
  detail = await request(`/marketplace/runs/${run.id}`, {}, token);
  if (["succeeded", "archived", "cancelled", "fatal", "failed"].includes(detail.run.status)) break;
  await new Promise((resolve) => setTimeout(resolve, 5000));
}
if (!detail || !["succeeded", "archived", "cancelled", "fatal", "failed"].includes(detail.run.status)) {
  throw new Error("manual report polling exceeded the 15-minute gate timeout");
}
const analysis = detail.analyses.at(0)?.result ?? null;
const result = {
  gate: "amazon-staging-first-report",
  approval_reference: gate.approval_reference,
  requested_at_utc: startedAt.toISOString(),
  completed_at_utc: new Date().toISOString(),
  report_type: detail.run.report_type,
  marketplace_id: gate.marketplace_id,
  data_start_time: start.toISOString(),
  data_end_time: end.toISOString(),
  seller_id_redacted: connection.seller_id_redacted,
  region: connection.region,
  roles: connection.granted_roles,
  report_status: detail.run.status,
  polling_duration_seconds: Math.round((Date.now() - startedAt.valueOf()) / 1000),
  rate_limit: detail.transport.map((entry) => ({
    operation: entry.operation,
    request_id_redacted: entry.request_id_redacted,
    limit: entry.rate_limit_limit,
    retry_after_seconds: entry.retry_after_seconds,
  })),
  document_size: detail.document ? {
    transport_bytes: detail.document.transport_bytes,
    decoded_bytes: detail.document.decoded_bytes,
  } : null,
  transport_sha256: detail.document?.sha256 ?? null,
  decoded_sha256: detail.document?.decoded_sha256 ?? null,
  parser_version: detail.document?.parser_version ?? null,
  normalized_metric_records: detail.metrics.length,
  missing_fields: analysis?.missing_data ?? [],
  freshness_seconds: detail.run.completed_at
    ? Math.max(0, Math.round((new Date(detail.run.completed_at).valueOf() - end.valueOf()) / 1000))
    : null,
  analysis: analysis ? {
    trend: analysis.overall_trend ?? analysis.trend ?? null,
    uncertainty: analysis.uncertainty ?? null,
    anomalies_count: Array.isArray(analysis.anomalies) ? analysis.anomalies.length : null,
    hypotheses_count: Array.isArray(analysis.hypotheses) ? analysis.hypotheses.length : null,
    suggested_actions_count: Array.isArray(analysis.options) ? analysis.options.length : null,
    evidence_refs: analysis.evidence_refs ?? [],
  } : null,
  raw_report_committed: false,
  credentials_recorded: false,
};
writeFileSync(args.output, `${JSON.stringify(result, null, 2)}\n`, { mode: 0o600, flag: "wx" });
process.stdout.write(`${JSON.stringify({ result: detail.run.status, output: args.output, credentials_recorded: false })}\n`);
if (detail.run.status !== "succeeded") process.exit(4);
