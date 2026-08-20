#!/usr/bin/env node

import { createHash } from "node:crypto";

const baseUrl = required("MANTLE_AMAZON_BASE_URL").replace(/\/+$/, "");
const username = required("MANTLE_AMAZON_ADMIN_USERNAME");
const password = required("MANTLE_AMAZON_ADMIN_PASSWORD");
const timezone = process.env.MANTLE_AMAZON_TIMEZONE || "Europe/Berlin";
const marketplace = "SYNTHETIC-MARKETPLACE";
const reportType = "GET_SALES_AND_TRAFFIC_REPORT";

function required(name) {
  const value = process.env[name];
  if (!value) throw new Error(`${name} must be set`);
  return value;
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

async function request(path, { method = "GET", token, body, contentType } = {}) {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(contentType ? { "Content-Type": contentType } : {}),
    },
    body,
  });
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (!response.ok) {
    throw new Error(`${method} ${path} failed with HTTP ${response.status}`);
  }
  return { response, bytes };
}

function syntheticReport(startDate, dailyRevenue, dailyUnits, dailySessions) {
  const start = new Date(`${startDate}T00:00:00Z`);
  const rows = Array.from({ length: 7 }, (_, offset) => {
    const date = new Date(start);
    date.setUTCDate(start.getUTCDate() + offset);
    const dateText = date.toISOString().slice(0, 10);
    return {
      date: dateText,
      salesByDate: {
        orderedProductSales: {
          amount: dailyRevenue.toFixed(2),
          currencyCode: "EUR",
        },
        orderedProductSalesB2B: {
          amount: (dailyRevenue / 10).toFixed(2),
          currencyCode: "EUR",
        },
        unitsOrdered: dailyUnits,
        unitsOrderedB2B: 1,
      },
      trafficByDate: {
        sessions: dailySessions,
        pageViews: dailySessions * 2,
        unitSessionPercentage: Number(
          ((dailyUnits / dailySessions) * 100).toFixed(4),
        ),
        buyBoxPercentage: 92,
      },
    };
  });
  const end = rows.at(-1).date;
  return Buffer.from(
    JSON.stringify({
      syntheticTestData: true,
      syntheticNotice: "SYNTHETIC TEST DATA - NO AMAZON BUSINESS DATA",
      reportSpecification: {
        reportType,
        reportOptions: { dateGranularity: "DAY", asinGranularity: "CHILD" },
        dataStartTime: startDate,
        dataEndTime: end,
        marketplaceIds: [marketplace],
      },
      salesAndTrafficByDate: rows,
      salesAndTrafficByAsin: [],
    }),
  );
}

function query(filename, confirmation = {}) {
  return new URLSearchParams({ filename, timezone, ...confirmation }).toString();
}

async function preview(token, filename, raw) {
  const result = await request(
    `/api/marketplace/imports/preview?${query(filename)}`,
    {
      method: "POST",
      token,
      body: raw,
      contentType: "application/octet-stream",
    },
  );
  return JSON.parse(Buffer.from(result.bytes).toString("utf8"));
}

async function importReport(token, filename, raw, parsed) {
  const confirmation = {
    confirm_hash: parsed.sha256,
    confirm_marketplace_id: parsed.marketplace_id,
    confirm_currency_code: parsed.currency_code,
    confirm_period_start: parsed.period_start,
    confirm_period_end: parsed.period_end,
    confirm_granularity: parsed.granularity,
    confirm_report_type: parsed.report_type,
  };
  const result = await request(
    `/api/marketplace/imports?${query(filename, confirmation)}`,
    {
      method: "POST",
      token,
      body: raw,
      contentType: "application/octet-stream",
    },
  );
  return JSON.parse(Buffer.from(result.bytes).toString("utf8"));
}

const login = await request("/api/auth/login", {
  method: "POST",
  contentType: "application/json",
  body: JSON.stringify({ username, password }),
});
const token = JSON.parse(Buffer.from(login.bytes).toString("utf8")).access_token;
if (!token) throw new Error("login response did not contain an access token");

// Upload the newer period first to prove comparison selection is based on report
// periods, not import order. The exact synthetic reports exist only in memory.
const newerRaw = syntheticReport("2026-07-08", 20, 2, 20);
const olderRaw = syntheticReport("2026-07-01", 10, 1, 20);
const newerPreview = await preview(token, "SYNTHETIC-sales-traffic-newer.json", newerRaw);
const newerFirst = await importReport(
  token,
  "SYNTHETIC-sales-traffic-newer.json",
  newerRaw,
  newerPreview,
);
const newerRepeat = await importReport(
  token,
  "SYNTHETIC-sales-traffic-newer.json",
  newerRaw,
  newerPreview,
);
const olderPreview = await preview(token, "SYNTHETIC-sales-traffic-older.json", olderRaw);
const olderSecond = await importReport(
  token,
  "SYNTHETIC-sales-traffic-older.json",
  olderRaw,
  olderPreview,
);

const flatFixtures = [
  {
    format: "csv",
    filename: "SYNTHETIC-sales-traffic.csv",
    raw: Buffer.from(
      "Date,Marketplace ID,Ordered Product Sales,Currency,Units Ordered,Sessions,Page Views,Unit Session Percentage,Buy Box Percentage\n" +
      "2026-06-01,SYNTHETIC-CSV-MARKETPLACE,10.00,EUR,1,10,20,10,90\n",
    ),
  },
  {
    format: "tsv",
    filename: "SYNTHETIC-sales-traffic.tsv",
    raw: Buffer.from(
      "Datum\tMarktplatz-ID\tUmsatz bestellter Produkte\tWährung\tBestellte Einheiten\tSitzungen\tSeitenaufrufe\tProzentsatz der Einheiten pro Sitzung\tBuy Box Percentage\n" +
      "2026-06-02\tSYNTHETIC-TSV-MARKETPLACE\t20,00\tEUR\t2\t20\t40\t10,00%\t91,00%\n",
    ),
  },
];
const flatImports = {};
for (const fixture of flatFixtures) {
  const parsed = await preview(token, fixture.filename, fixture.raw);
  if (parsed.detected_format !== fixture.format || parsed.confirmation_required) {
    throw new Error(`${fixture.format} report was not previewed as a complete Sales and Traffic report`);
  }
  const imported = await importReport(token, fixture.filename, fixture.raw, parsed);
  if (!["imported", "already_imported"].includes(imported.outcome)) {
    throw new Error(`${fixture.format} report was not imported atomically`);
  }
  flatImports[fixture.format] = {
    sha256: sha256(fixture.raw),
    run_id: imported.run_id,
  };
}

if (
  !["imported", "already_imported"].includes(newerFirst.outcome) ||
  newerFirst.comparison_generated
) {
  throw new Error("first period did not produce the expected comparison baseline");
}
if (
  newerRepeat.outcome !== "already_imported" ||
  newerRepeat.run_id !== newerFirst.run_id
) {
  throw new Error("repeated import was not idempotent");
}
if (
  !["imported", "already_imported"].includes(olderSecond.outcome) ||
  !olderSecond.comparison_generated ||
  !olderSecond.analysis_id
) {
  throw new Error("second compatible period did not produce a comparison");
}

const exportHashes = {};
for (const format of ["json", "markdown", "csv"]) {
  const exported = await request(
    `/api/marketplace/analyses/${olderSecond.analysis_id}/export?format=${format}`,
    { token },
  );
  const exportText = Buffer.from(exported.bytes).toString("utf8");
  for (const forbidden of [
    "salesAndTrafficByDate",
    "syntheticNotice",
    "SYNTHETIC TEST DATA - NO AMAZON BUSINESS DATA",
  ]) {
    if (exportText.includes(forbidden)) {
      throw new Error(`${format} export leaked a raw-report field`);
    }
  }
  if (format === "json") {
    const result = JSON.parse(exportText).result;
    const factNames = new Set(result.facts.map((fact) => fact.metric));
    for (const requiredMetric of [
      "ordered_product_sales",
      "units_ordered",
      "sessions",
      "page_views",
      "conversion_rate",
      "buy_box_percentage",
      "b2b_revenue_share",
    ]) {
      if (!factNames.has(requiredMetric)) {
        throw new Error(`comparison export lacks ${requiredMetric}`);
      }
    }
    if (
      result.context.marketplace !== marketplace ||
      result.context.parser_version !== "manual-sales-traffic-v1" ||
      result.context.source_timezone !== timezone ||
      result.context.currency !== "EUR"
    ) {
      throw new Error("comparison context lost confirmed provenance");
    }
    if (
      !Array.isArray(result.changes_since_last_run) ||
      result.changes_since_last_run.length === 0 ||
      !result.changes_since_last_run.every(
        (change) =>
          "current" in change &&
          "previous" in change &&
          "difference" in change &&
          "percent_change" in change &&
          "trend" in change,
      )
    ) {
      throw new Error("comparison lacks absolute, delta, percentage, or trend fields");
    }
    for (const category of [
      "derived_observations",
      "hypotheses",
      "options",
      "missing_evidence",
      "open_questions",
    ]) {
      if (!Array.isArray(result[category]) || result[category].length === 0) {
        throw new Error(`comparison lacks visible ${category}`);
      }
    }
    if (!result.uncertainty || !Array.isArray(result.anomalies)) {
      throw new Error("comparison lacks uncertainty or anomaly classification");
    }
  }
  exportHashes[format] = sha256(exported.bytes);
}

for (const { method, path, body } of [
  { method: "GET", path: `/api/marketplace/runs/${newerFirst.run_id}/raw` },
  { method: "POST", path: "/api/articles/", body: "{}" },
  {
    method: "PUT",
    path: "/api/marketplace/connections/synthetic/schedules",
    body: "{}",
  },
]) {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      Authorization: `Bearer ${token}`,
      ...(body ? { "Content-Type": "application/json" } : {}),
    },
    body,
  });
  await response.arrayBuffer();
  if (response.status !== 409) {
    throw new Error(`${method} ${path} was not blocked by the read-only pilot`);
  }
}

process.stdout.write(
  `${JSON.stringify(
    {
      kind: "synthetic_manual_amazon_acceptance",
      raw_reports_written_to_disk: false,
      report_type: reportType,
      formats: ["json", "csv", "tsv"],
      newer_sha256: sha256(newerRaw),
      older_sha256: sha256(olderRaw),
      baseline_run_id: newerFirst.run_id,
      idempotent_run_id: newerRepeat.run_id,
      comparison_run_id: olderSecond.run_id,
      comparison_analysis_id: olderSecond.analysis_id,
      flat_imports: flatImports,
      export_sha256: exportHashes,
      raw_download_blocked: true,
      business_mutations_blocked: true,
    },
    null,
    2,
  )}\n`,
);
