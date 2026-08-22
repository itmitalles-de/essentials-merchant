#!/usr/bin/env node

import { createHash } from "node:crypto";

const baseUrl = required("MANTLE_AMAZON_BASE_URL").replace(/\/+$/, "");
const username = process.env.MANTLE_AMAZON_ADMIN_USERNAME;
const password = process.env.MANTLE_AMAZON_ADMIN_PASSWORD;
const timezone = process.env.MANTLE_AMAZON_TIMEZONE || "Europe/Berlin";
const marketplace = "SYNTHETIC-MARKETPLACE";
const reportType = "GET_SALES_AND_TRAFFIC_REPORT";
const adsReportType = "AMAZON_ADS_SPONSORED_PRODUCTS_CAMPAIGN_REPORT";

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

function syntheticAdsReport(startDate, endDate, suffix, spend, attributedSales) {
  return Buffer.from(
    "Start Date,End Date,Marketplace ID,Ad Product,Campaign Name,Impressions,Clicks,Spend,Currency,14 Day Total Sales,14 Day Total Orders (#),14 Day Total Units (#)\n" +
      `${startDate},${endDate},${marketplace},Sponsored Products,SYNTHETIC-ADS-${suffix},1000,50,EUR ${spend.toFixed(2)},EUR,EUR ${attributedSales.toFixed(2)},8,10\n`,
  );
}

async function previewAds(token, filename, raw) {
  const result = await request(
    `/api/marketplace/imports/ads/preview?${query(filename)}`,
    {
      method: "POST",
      token,
      body: raw,
      contentType: "application/octet-stream",
    },
  );
  return JSON.parse(Buffer.from(result.bytes).toString("utf8"));
}

async function importAds(token, filename, raw, parsed) {
  const confirmation = {
    confirm_hash: parsed.sha256,
    confirm_marketplace_id: parsed.marketplace_id,
    confirm_currency_code: parsed.currency_code,
    confirm_period_start: parsed.period_start,
    confirm_period_end: parsed.period_end,
    confirm_granularity: parsed.granularity,
    confirm_report_type: parsed.report_type,
    confirm_attribution_window_days: parsed.attribution_window_days,
  };
  const result = await request(
    `/api/marketplace/imports/ads?${query(filename, confirmation)}`,
    {
      method: "POST",
      token,
      body: raw,
      contentType: "application/octet-stream",
    },
  );
  return JSON.parse(Buffer.from(result.bytes).toString("utf8"));
}

async function authenticate() {
  try {
    const session = await request("/api/auth/pilot-session", { method: "POST" });
    const token = JSON.parse(Buffer.from(session.bytes).toString("utf8")).access_token;
    if (token) return token;
  } catch {
    // Non-Mantle deployments may still use the regular administrator login.
  }
  if (!username || !password) {
    throw new Error(
      "pilot session unavailable and MANTLE_AMAZON_ADMIN_USERNAME/PASSWORD are not set",
    );
  }
  const login = await request("/api/auth/login", {
    method: "POST",
    contentType: "application/json",
    body: JSON.stringify({ username, password }),
  });
  const token = JSON.parse(Buffer.from(login.bytes).toString("utf8")).access_token;
  if (!token) throw new Error("authentication response did not contain an access token");
  return token;
}

const token = await authenticate();

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

// Aggregate Sponsored Products evidence follows the same immutable receipt
// boundary. Campaign labels exist only in these in-memory synthetic raw bytes.
const newerAdsRaw = syntheticAdsReport("2026-07-15", "2026-07-21", "NEWER", 25, 100);
const olderAdsRaw = syntheticAdsReport("2026-07-08", "2026-07-14", "OLDER", 20, 60);
const newerAdsPreview = await previewAds(
  token,
  "SYNTHETIC-sponsored-products-newer.csv",
  newerAdsRaw,
);
const serializedAdsPreview = JSON.stringify(newerAdsPreview);
if (
  newerAdsPreview.report_type !== adsReportType ||
  newerAdsPreview.attribution_window_days !== 14 ||
  newerAdsPreview.confirmation_required ||
  serializedAdsPreview.includes("SYNTHETIC-ADS-NEWER")
) {
  throw new Error("Ads preview lost report, attribution, confirmation, or identifier boundaries");
}
const adsMetrics = new Map(
  newerAdsPreview.metrics.map((metric) => [metric.metric_name, metric.value_numeric]),
);
for (const requiredMetric of ["ads_impressions", "ads_clicks", "ads_spend", "ads_ctr", "ads_cpc", "ads_roas", "ads_acos"]) {
  if (!adsMetrics.has(requiredMetric)) {
    throw new Error(`Ads preview lacks ${requiredMetric}`);
  }
}
const newerAdsFirst = await importAds(
  token,
  "SYNTHETIC-sponsored-products-newer.csv",
  newerAdsRaw,
  newerAdsPreview,
);
const newerAdsRepeat = await importAds(
  token,
  "SYNTHETIC-sponsored-products-newer.csv",
  newerAdsRaw,
  newerAdsPreview,
);
const olderAdsPreview = await previewAds(
  token,
  "SYNTHETIC-sponsored-products-older.csv",
  olderAdsRaw,
);
const olderAdsSecond = await importAds(
  token,
  "SYNTHETIC-sponsored-products-older.csv",
  olderAdsRaw,
  olderAdsPreview,
);
if (
  !["imported", "already_imported"].includes(newerAdsFirst.outcome) ||
  newerAdsRepeat.outcome !== "already_imported" ||
  newerAdsRepeat.run_id !== newerAdsFirst.run_id ||
  !["imported", "already_imported"].includes(olderAdsSecond.outcome) ||
  !olderAdsSecond.comparison_generated ||
  !olderAdsSecond.analysis_id
) {
  throw new Error("Ads import did not prove baseline, idempotence, and period comparison");
}

const adsExportHashes = {};
for (const format of ["json", "markdown", "csv"]) {
  const exported = await request(
    `/api/marketplace/analyses/${olderAdsSecond.analysis_id}/export?format=${format}`,
    { token },
  );
  const exportText = Buffer.from(exported.bytes).toString("utf8");
  for (const forbidden of ["SYNTHETIC-ADS-NEWER", "SYNTHETIC-ADS-OLDER", "campaignId", "campaignName"]) {
    if (exportText.includes(forbidden)) {
      throw new Error(`${format} Ads export leaked a campaign identifier`);
    }
  }
  if (format === "json") {
    const result = JSON.parse(exportText).result;
    const factNames = new Set(result.facts.map((fact) => fact.metric));
    for (const requiredMetric of ["ads_impressions", "ads_clicks", "ads_spend", "ads_roas", "ads_acos"]) {
      if (!factNames.has(requiredMetric)) {
        throw new Error(`Ads comparison export lacks ${requiredMetric}`);
      }
    }
  }
  adsExportHashes[format] = sha256(exported.bytes);
}

const rejectedAds = Buffer.from(
  "Date,Campaign Name,Search Term,Impressions,Clicks,Spend\n" +
    "2026-07-22,SYNTHETIC-REJECTED,SYNTHETIC-TERM,10,1,1.00\n",
);
const rejectedResponse = await fetch(
  `${baseUrl}/api/marketplace/imports/ads/preview?${query("SYNTHETIC-search-term-rejected.csv")}`,
  {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/octet-stream",
    },
    body: rejectedAds,
  },
);
await rejectedResponse.arrayBuffer();
if (rejectedResponse.status !== 422) {
  throw new Error("search-term Ads report was not rejected before persistence");
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
      ads: {
        report_type: adsReportType,
        formats: ["csv"],
        newer_sha256: sha256(newerAdsRaw),
        older_sha256: sha256(olderAdsRaw),
        baseline_run_id: newerAdsFirst.run_id,
        idempotent_run_id: newerAdsRepeat.run_id,
        comparison_run_id: olderAdsSecond.run_id,
        comparison_analysis_id: olderAdsSecond.analysis_id,
        aggregate_metrics_verified: [...adsMetrics.keys()].sort(),
        export_sha256: adsExportHashes,
        campaign_identifiers_excluded: true,
        search_term_report_rejected: true,
      },
      raw_download_blocked: true,
      business_mutations_blocked: true,
    },
    null,
    2,
  )}\n`,
);
