import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

const uiMarketplace = "SYNTHETIC-UI-MARKETPLACE";

function syntheticManualReport(date: string, revenue: number, units: number, sessions: number) {
  return Buffer.from(JSON.stringify({
    syntheticTestData: true,
    reportSpecification: {
      reportType: "GET_SALES_AND_TRAFFIC_REPORT",
      reportOptions: { dateGranularity: "DAY", asinGranularity: "CHILD" },
      dataStartTime: date,
      dataEndTime: date,
      marketplaceIds: [uiMarketplace],
    },
    salesAndTrafficByDate: [{
      date,
      salesByDate: {
        orderedProductSales: { amount: revenue.toFixed(2), currencyCode: "EUR" },
        unitsOrdered: units,
      },
      trafficByDate: {
        sessions,
        pageViews: sessions * 2,
        unitSessionPercentage: (units / sessions) * 100,
        buyBoxPercentage: 90,
      },
    }],
    salesAndTrafficByAsin: [],
  }));
}

test("scoped Mantle session completes the synthetic read-only Amazon pilot flow", async ({ page }) => {
  await page.goto("/ai-marketing");

  await expect(page.getByTestId("pilot-banner")).toHaveCount(0);
  await expect(page.getByRole("link", { name: "Einstellungen öffnen" })).toBeVisible();

  const pilotStatus = await page.evaluate(async () => {
    const token = localStorage.getItem("erplite-token");
    const response = await fetch("/api/pilot/status", {
      headers: token ? { authorization: `Bearer ${token}` } : {},
    });
    return { status: response.status, body: await response.json() };
  });
  expect(pilotStatus.status).toBe(200);
  expect(JSON.stringify(pilotStatus.body)).toContain("amazon-read-only");

  // localhost is used only by the full-flow E2E stack. The production hostname
  // remains locked to /ai-marketing by App.tsx.
  await page.goto("/marketplace");
  await expect(page.getByRole("heading", { name: "Amazon Intelligence" })).toBeVisible();
  const demoButton = page.getByRole("button", { name: "Synthetische Demo einrichten" });
  const syntheticConnection = page.getByText(/Verbindung: Synthetische Demo/);
  await expect(demoButton.or(syntheticConnection)).toBeVisible();
  if (await demoButton.isVisible()) await demoButton.click();
  await expect(syntheticConnection).toBeVisible();
  await expect(page.getByText(/Verbindungsstatus: synthetisch – kein Amazonzugang/)).toBeVisible();
  await expect(page.getByText(/Secret-Shape: nicht erforderlich \(Fixture\)/)).toBeVisible();

  await page.getByRole("button", { name: "Sales & Traffic jetzt abrufen" }).click();
  await expect(page.getByRole("heading", { name: /Reportlauf/ })).toBeVisible();
  await expect(page.getByText(/Job: succeeded/)).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText(/Roharchiv: unveränderlich/)).toBeVisible();
  await expect(page.getByText(/Snapshot: day_child/)).toBeVisible();
  await expect(page.getByRole("heading", { name: "Delta-Analyse" })).toBeVisible();
  const defaultStrategyPanel = page.locator(".strategy-panel").first();
  await expect(defaultStrategyPanel).toContainText("Analyse noch nicht ausführbar");
  await expect(defaultStrategyPanel.getByRole("button", { name: "Analyse", exact: true }))
    .toBeDisabled();

  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "PII-minimierten Analyseexport laden" }).first().click();
  expect((await download).suggestedFilename()).toMatch(/^marketplace-analysis-.*\.json$/);

  const upload = async (name: string, bytes: Buffer) => {
    await page.getByLabel("Offizieller Amazon-Report").setInputFiles({
      name,
      mimeType: "application/json",
      buffer: bytes,
    });
    await page.getByLabel("Report-Zeitzone").fill("Europe/Berlin");
    await page.getByRole("button", { name: "Importvorschau erstellen" }).click();
    await expect(page.getByRole("heading", { name: "Geprüfte Importvorschau" })).toBeVisible();
    await expect(page.getByText(uiMarketplace, { exact: true }).first()).toBeVisible();
    await page.getByLabel(/Hash, Reporttyp, Marketplace/).check();
    await page.getByRole("button", { name: "Bestätigten Import ausführen" }).click();
    await expect(page.locator(".marketplace-callout.success")).toBeVisible();
  };

  // Newer first proves that the complete operator flow compares report periods,
  // not upload order. The synthetic bytes are supplied from memory only.
  await upload(
    "SYNTHETIC-ui-newer.json",
    syntheticManualReport("2026-05-08", 20, 2, 20),
  );
  await page.getByRole("button", { name: "Zweiten Zeitraum hinzufügen" }).click();
  await upload(
    "SYNTHETIC-ui-older.json",
    syntheticManualReport("2026-05-01", 10, 1, 20),
  );
  await expect(page.locator(".marketplace-callout.success")).toContainText("Vergleichsanalyse");

  const manualAnalysis = page.locator(".analysis-card").filter({ hasText: uiMarketplace }).first();
  await expect(manualAnalysis.getByText("KPI-Überblick")).toBeVisible();
  for (const heading of ["Fakten", "Belastbare Ableitungen", "Hypothesen", "Offene Fragen"]) {
    await expect(manualAnalysis.getByRole("heading", { name: heading })).toBeVisible();
  }
  for (const [label, extension] of [
    ["PII-minimierten Analyseexport laden", "json"],
    ["Markdown exportieren", "md"],
    ["CSV exportieren", "csv"],
  ] as const) {
    const exportDownload = page.waitForEvent("download");
    await manualAnalysis.getByRole("button", { name: label }).click();
    expect((await exportDownload).suggestedFilename()).toMatch(new RegExp(`\\.${extension}$`));
  }

  await page.goto("/ai-marketing");
  await expect(page.locator(".analysis-card")).toHaveCount(0);
  const adsCard = page.locator("section.card").filter({
    has: page.getByRole("heading", { name: "Read-only Amazon-Ads-Evidenz" }),
  });
  const uploadAds = async (name: string, start: string, end: string, spend: number, sales: number) => {
    const report = Buffer.from(
      "Start Date,End Date,Marketplace ID,Ad Product,Campaign Name,Impressions,Clicks,Spend,Currency,14 Day Total Sales,14 Day Total Orders (#),14 Day Total Units (#)\n"
      + `${start},${end},${uiMarketplace},Sponsored Products,SYNTHETIC-E2E,1000,50,EUR ${spend.toFixed(2)},EUR,EUR ${sales.toFixed(2)},8,10\n`,
    );
    await adsCard.getByLabel("Offizieller Ads-Kampagnenbericht").setInputFiles({
      name,
      mimeType: "text/csv",
      buffer: report,
    });
    await adsCard.getByLabel("Report-Zeitzone").fill("Europe/Berlin");
    await adsCard.getByRole("button", { name: "Ads-Importvorschau erstellen" }).click();
    await expect(adsCard.getByRole("heading", { name: "Geprüfte Ads-Vorschau" })).toBeVisible();
    await expect(adsCard.getByRole("table", { name: "Identifierfreie Ads-Aggregate" }))
      .toContainText("ads_roas");
    await expect(adsCard).not.toContainText("SYNTHETIC-E2E");
    await adsCard.getByLabel(/Hash, Kampagnenreport, Marketplace/).check();
    await adsCard.getByRole("button", { name: "Bestätigten Ads-Import ausführen" }).click();
    await expect(adsCard.locator(".marketplace-callout.success")).toBeVisible();
  };

  await uploadAds("SYNTHETIC-ads-older.csv", "2026-05-01", "2026-05-07", 20, 60);
  await adsCard.getByRole("button", { name: "Weiteren Ads-Zeitraum importieren" }).click();
  await uploadAds("SYNTHETIC-ads-newer.csv", "2026-05-08", "2026-05-14", 25, 100);
  await expect(adsCard.getByRole("status")).toContainText("Periodenvergleich erzeugt");
  await adsCard.getByRole("button", { name: "Weiteren Ads-Zeitraum importieren" }).click();
  await uploadAds("SYNTHETIC-ads-newer.csv", "2026-05-08", "2026-05-14", 25, 100);
  await expect(adsCard.getByRole("status")).toContainText("bereits unverändert importiert");

  let aggregateHash = "a".repeat(64);
  let rejectFirstStrategyPost = true;
  await page.route(/\/api\/marketplace\/strategy\/weekly$/, async (route) => {
    const base = {
      anchor_analysis_id: "66666666-6666-4666-8666-666666666666",
      current_payload_sha256: aggregateHash,
      assessment_payload_sha256: null,
      status: {
        available: true,
        reason: null,
        provider: "openai",
        model: "gpt-5.6",
        prompt_version: "mantle-amazon-weekly-strategy-v4",
        response_storage: "store_false",
        input_boundary: "separate_public_research_then_curated_business_context_aggregate_history_and_handover",
        cadence: "manual_weekly",
        calendar_timezone: "Europe/Berlin",
        automatic_execution: false,
        mutation_capability: false,
        public_web_research: true,
        max_web_search_calls: 3,
      },
      can_run: true,
      block_reason: null,
      week_start: "2026-08-17",
      next_available_at: "2026-08-23T22:00:00Z",
      source_analysis_count: 2,
      business_knowledge_imported: true,
      business_knowledge_source_count: 6,
      business_knowledge_entry_count: 18,
      business_knowledge_sha256: "c".repeat(64),
      previous_run_context: true,
      provider_request_id_redacted: null,
      input_tokens: null,
      output_tokens: null,
      assessment_week_start: null,
      assessment_model: null,
      assessment_prompt_version: null,
      created_at: null,
    };
    if (route.request().method() === "GET") {
      await route.fulfill({ json: { ...base, cached: false, assessment: null } });
      return;
    }
    const body = route.request().postDataJSON() as Record<string, unknown>;
    expect(body).toEqual({
      confirmed_payload_sha256: aggregateHash,
      confirmed_aggregate_only: true,
    });
    if (rejectFirstStrategyPost) {
      rejectFirstStrategyPost = false;
      aggregateHash = "b".repeat(64);
      await new Promise((resolve) => setTimeout(resolve, 400));
      await route.fulfill({
        status: 412,
        json: { error: "aggregate_confirmation_mismatch" },
      });
      return;
    }
    await route.fulfill({
      json: {
        ...base,
        can_run: false,
        block_reason: "weekly_limit_reached",
        cached: false,
        assessment_payload_sha256: aggregateHash,
        input_tokens: 120,
        output_tokens: 60,
        assessment_week_start: "2026-08-17",
        assessment_model: "gpt-5.6",
        assessment_prompt_version: "mantle-amazon-weekly-strategy-v4",
        created_at: "2026-08-20T12:00:00Z",
        assessment: {
          executive_summary: "Synthetische KI-Zusammenfassung ohne Geschäftsdaten.",
          assessment: "Die Conversion sollte mit zusätzlicher Evidenz geprüft werden.",
          opportunities: [{
            title: "Synthetische Chance",
            rationale: "Der Periodenvergleich zeigt ein messbares Signal.",
            confidence: "medium",
            evidence_refs: ["analysis:1:fact:sessions"],
          }],
          risks: [],
          hypotheses: [{
            statement: "Eine Kampagne könnte beigetragen haben.",
            rationale: "Ads-Daten fehlen.",
            confidence: "low",
            evidence_needed: ["Ads-Bericht desselben Zeitraums"],
            evidence_refs: [],
          }],
          recommended_actions: [{
            title: "Evidenz abgleichen",
            rationale: "Kausalität ist nicht belegt.",
            priority: "now",
            expected_signal: "Zeitlich übereinstimmende Veränderung",
            risks: ["Scheinkorrelation"],
            evidence_refs: [],
          }],
          public_context: {
            competitor_signals: [{
              title: "Synthetisches Wettbewerbersignal",
              observed_fact: "Eine synthetische öffentliche Quelle zeigt ein Marktangebot.",
              possible_consumption_impact: "Die Vergleichsintensität könnte steigen.",
              confidence: "medium",
              uncertainty: "Keine Wirkung auf interne Zahlen ist belegt.",
              evidence_refs: ["public:1"],
            }],
            category_trends: [{
              title: "Synthetischer Kategorietrend",
              observed_fact: "Eine synthetische Quelle beschreibt die Kategorie.",
              possible_consumption_impact: "Die Preisempfindlichkeit könnte sich verändern.",
              confidence: "low",
              uncertainty: "Die Kategorieübertragung ist unsicher.",
              evidence_refs: ["public:2"],
            }],
            global_events_and_crises: [{
              title: "Synthetisches globales Signal",
              observed_fact: "Eine synthetische Institution meldet ein globales Risiko.",
              possible_consumption_impact: "Diskretionärer Konsum könnte zurückgehen.",
              confidence: "low",
              uncertainty: "Keine Kausalität zu Amazon-Daten.",
              evidence_refs: ["public:3"],
            }],
          },
          public_sources: [
            { ref: "public:1", title: "Quelle Wettbewerb", url: "https://example.test/rival" },
            { ref: "public:2", title: "Quelle Markt", url: "https://example.test/market" },
            { ref: "public:3", title: "Quelle Krise", url: "https://example.test/crisis" },
          ],
          open_questions: ["Welche Kampagnen liefen?"],
          limitations: ["Keine Ads-, Preis- oder Bestandsdaten."],
          handover: {
            continuity_summary: "Traffic-Evidenz bleibt für den nächsten Wochenlauf offen.",
            priorities_until_next_run: ["Ads-Evidenz sammeln"],
            evidence_for_next_run: ["Aggregierter Ads-Bericht"],
            next_run_checks: ["Sessions und Conversion erneut vergleichen"],
          },
        },
      },
    });
  });
  await page.reload();
  const strategyPanel = page.locator(".strategy-panel").first();
  await expect(page.locator(".strategy-panel")).toHaveCount(1);
  await expect(strategyPanel.getByLabel("Ablauf der wöchentlichen Analyse")).toBeVisible();
  for (const phase of [
    "Amazon-Report",
    "Validierung & KPIs",
    "Markt & Wettbewerb",
    "Globale Krisen",
    "Strategie & Handover",
  ]) {
    await expect(strategyPanel.getByText(phase, { exact: true })).toBeVisible();
  }
  await expect(strategyPanel).toContainText(aggregateHash);
  const strategyButton = strategyPanel.getByRole("button", { name: "Analyse", exact: true });
  await strategyButton.click();
  await expect(strategyPanel.getByLabel("Ablauf der wöchentlichen Analyse"))
    .toHaveAttribute("aria-busy", "true");
  await expect(strategyPanel).toContainText("b".repeat(64));
  await expect(strategyButton).toBeEnabled();
  await strategyButton.click();
  await expect(strategyButton).toBeDisabled();
  await expect(strategyPanel.getByText("KI-generiert – keine Faktenquelle")).toBeVisible();
  await expect(strategyPanel).toContainText("Synthetische Chance");
  await expect(strategyPanel).toContainText("Globale Trends und Krisen");
  await expect(strategyPanel.getByRole("link", { name: "Quelle Krise" })).toHaveAttribute("href", "https://example.test/crisis");
  await expect(strategyPanel).toContainText("Hypothesen – nicht als Fakten behandeln");
  await expect(strategyPanel).toContainText("Handover bis zum nächsten Wochenlauf");
  await expect(strategyPanel).toContainText("Wochenlimit aktiv");
  await expect(page.locator(".analysis-card")).toHaveCount(0);

  const statuses = await page.evaluate(async () => {
    const requests: Array<[string, string]> = [
      ["POST", "/api/articles/"],
      ["POST", "/api/sales-orders/"],
      ["POST", "/api/sales-orders/00000000-0000-0000-0000-000000000000/fulfill"],
      ["POST", "/api/exports/datev"],
      ["PUT", "/api/modules/payment.test"],
      ["POST", "/api/integrations/vendure/orders"],
      ["GET", "/api/marketplace/runs/00000000-0000-0000-0000-000000000000/raw"],
      ["GET", "/api/modules/shipping.dhl/health"],
    ];
    const token = localStorage.getItem("erplite-token");
    return Promise.all(requests.map(async ([method, path]) => {
      const response = await fetch(path, {
        method,
        headers: {
          "content-type": "application/json",
          ...(token ? { authorization: `Bearer ${token}` } : {}),
        },
        ...(method === "GET" ? {} : { body: "{}" }),
      });
      return response.status;
    }));
  });
  expect(statuses).toEqual([409, 409, 409, 409, 409, 409, 409, 409]);

  await page.goto("/ai-marketing");
  await expect(page.getByRole("heading", { name: "Amazon AI Marketing" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Wöchentliche KI-Marketinganalyse" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Zugänge" })).toHaveCount(0);
  await expect(page.locator("body")).not.toContainText(/refresh_token|client_secret|access_token/i);
  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(
    accessibility.violations.filter((violation) =>
      violation.impact === "serious" || violation.impact === "critical"),
  ).toEqual([]);
});

test("Mantle route opens without login and provider values stay write-only", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem("erplite-token", "stale-token-must-be-replaced");
  });
  await page.goto("/ai-marketing");
  await expect(page).toHaveTitle("Mantle · AI Marketing");
  await expect(page.locator('link[rel="icon"]')).toHaveAttribute("href", "/ai-marketing-icon.svg");
  await expect(page.getByRole("heading", { name: "Amazon AI Marketing" })).toBeVisible();
  await expect(page.locator('img[src="/ai-marketing-icon.svg"]')).toHaveCount(2);
  await expect(page.getByRole("heading", { name: "Zugänge" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Abmelden" })).toHaveCount(0);
  const themeSwitch = page.getByTestId("theme-switch");
  await expect(themeSwitch).toBeVisible();
  const originalTheme = await themeSwitch.getAttribute("aria-checked");
  await themeSwitch.click();
  await expect(themeSwitch).not.toHaveAttribute("aria-checked", originalTheme ?? "false");
  await expect(page.locator("html")).toHaveAttribute("data-theme", /^(light|dark)$/);

  const scopedStatuses = await page.evaluate(async () => {
    const token = localStorage.getItem("erplite-token");
    const headers = token ? { authorization: `Bearer ${token}` } : {};
    return Promise.all([
      fetch("/api/marketplace", { headers }).then((response) => response.status),
      fetch("/api/customers", { headers }).then((response) => response.status),
      fetch("/api/invoices", { headers }).then((response) => response.status),
    ]);
  });
  expect(scopedStatuses).toEqual([200, 403, 403]);

  const loginStatus = await page.evaluate(async () => (
    fetch("/api/auth/login", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ username: "admin", password: "ci-placeholder" }),
    }).then((response) => response.status)
  ));
  expect(loginStatus).toBe(403);

  await page.getByRole("link", { name: "Einstellungen öffnen" }).click();
  await expect(page).toHaveURL(/\/ai-marketing\/settings$/);
  await expect(page.getByRole("heading", { name: "Einstellungen" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Zugänge" })).toBeVisible();
  await expect(page.getByText("Read-only-Systemgrenze aktiv")).toBeVisible();

  const syntheticKey = `sk-proj-${"synthetic".repeat(4)}`;
  await page.getByLabel("Neuer Project API-Key").fill(syntheticKey);
  await page.getByLabel("Separates API-Pay-per-use-Budget ist eingerichtet.").check();
  const stored = page.waitForResponse(/\/api\/pilot\/provider-secrets\/openai$/);
  await page.getByRole("button", { name: "OpenAI-Key setzen/ersetzen" }).click();
  const responseBody = await (await stored).text();
  expect(responseBody).not.toContain(syntheticKey);
  await expect(page.getByText(/OpenAI-Zugang wurde gespeichert/)).toBeVisible();
  await expect(page.getByLabel("Neuer Project API-Key")).toHaveValue("");
  await page.reload();
  await expect(page.locator(".provider-form").first()).toContainText("konfiguriert");
  await expect(page.locator("body")).not.toContainText(syntheticKey);

  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(
    accessibility.violations.filter((violation) =>
      violation.impact === "serious" || violation.impact === "critical"),
  ).toEqual([]);
});

test("Mantle shell hides acceptance data and renders truthful live pipeline states", async ({ page }) => {
  const aggregateHash = "c".repeat(64);
  await page.route(/\/api\/marketplace$/, async (route) => {
    await route.fulfill({
      json: {
        connections: [],
        schedules: [],
        recent_runs: [{
          id: "11111111-1111-4111-8111-111111111111",
          connection_id: "22222222-2222-4222-8222-222222222222",
          schedule_id: null,
          marketplace_id: "SYNTHETIC-ACCEPTANCE",
          report_type: "GET_SALES_AND_TRAFFIC_REPORT",
          data_start_time: null,
          data_end_time: null,
          report_options: {},
          trigger_source: "manual",
          status: "succeeded",
          attempts: 1,
          poll_attempts: 0,
          next_attempt_at: null,
          amazon_report_id: null,
          amazon_report_document_id: null,
          failure_code: null,
          failure_message: null,
          requested_at: null,
          completed_at: "2026-08-20T12:00:00Z",
          created_at: "2026-08-20T12:00:00Z",
          updated_at: "2026-08-20T12:00:00Z",
        }],
        analyses: [{
          id: "33333333-3333-4333-8333-333333333333",
          job_id: "44444444-4444-4444-8444-444444444444",
          strategy: "deterministic_rules",
          model_name: null,
          prompt_version: "rules-v1",
          payload_sha256: "d".repeat(64),
          result: { context: { marketplace: "SYNTHETIC-ACCEPTANCE" }, facts: [] },
          created_at: "2026-08-20T12:00:00Z",
        }],
        report_types: [],
      },
    });
  });
  const base = {
    anchor_analysis_id: "55555555-5555-4555-8555-555555555555",
    current_payload_sha256: aggregateHash,
    assessment_payload_sha256: null,
    status: {
      available: true,
      reason: null,
      provider: "openai",
      model: "gpt-5.6",
      prompt_version: "mantle-amazon-weekly-strategy-v4",
      response_storage: "store_false",
      input_boundary: "separate_public_research_then_curated_business_context_aggregate_history_and_handover",
      cadence: "manual_weekly",
      calendar_timezone: "Europe/Berlin",
      automatic_execution: false,
      mutation_capability: false,
      public_web_research: true,
      max_web_search_calls: 3,
    },
    can_run: true,
    block_reason: null,
    week_start: "2026-08-17",
    next_available_at: "2026-08-23T22:00:00Z",
    source_analysis_count: 1,
    business_knowledge_imported: true,
    business_knowledge_source_count: 6,
    business_knowledge_entry_count: 18,
    business_knowledge_sha256: "c".repeat(64),
    previous_run_context: false,
    cached: false,
    assessment: null,
    assessment_week_start: null,
    assessment_model: null,
    assessment_prompt_version: null,
    provider_request_id_redacted: null,
    input_tokens: null,
    output_tokens: null,
    created_at: null,
  };
  await page.route(/\/api\/marketplace\/strategy\/weekly$/, async (route) => {
    if (route.request().method() === "GET") {
      await route.fulfill({ json: base });
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
    await route.fulfill({
      json: {
        ...base,
        can_run: false,
        block_reason: "weekly_limit_reached",
        assessment_payload_sha256: aggregateHash,
        created_at: "2026-08-20T12:30:00Z",
      },
    });
  });

  await page.goto("/ai-marketing");
  await expect(page.locator(".analysis-card")).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Zugänge" })).toHaveCount(0);
  const analysisPanel = page.getByRole("heading", { name: "Wöchentliche KI-Marketinganalyse" });
  const manualImport = page.getByRole("heading", { name: "Manueller Sales-&-Traffic-Import" });
  const [analysisBox, importBox] = await Promise.all([
    analysisPanel.boundingBox(),
    manualImport.boundingBox(),
  ]);
  expect(analysisBox).not.toBeNull();
  expect(importBox).not.toBeNull();
  expect(analysisBox!.y).toBeLessThan(importBox!.y);
  await expect(page.getByRole("cell", { name: "Noch keine Reportläufe." })).toBeVisible();
  const pipeline = page.getByLabel("Ablauf der wöchentlichen Analyse");
  const button = page.getByRole("button", { name: "Analyse", exact: true });
  await button.click();
  await expect(pipeline).toHaveAttribute("aria-busy", "true");
  const activity = page.getByRole("region", { name: "Bereinigtes Live-Protokoll" });
  await expect(activity).toBeVisible();
  await expect(activity).toContainText("Markt-, Wettbewerbs- und Krisensignale");
  await expect(activity).toContainText("store=false");
  await expect(activity).not.toContainText(/client_secret|refresh_token|access_token/i);
  await expect(pipeline.locator(".strategy-pipeline-stage.is-active")).toHaveCount(3);
  await expect(pipeline).toContainText("Globale Krisen");
  await expect(button).toBeDisabled();
  await expect(pipeline).toHaveAttribute("aria-busy", "false");
  await expect(pipeline.locator(".strategy-pipeline-stage.is-complete")).toHaveCount(5);
  await expect(activity).toContainText("Antwortschema, Evidenzreferenzen und Handover");
});
