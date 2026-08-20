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

  await expect(page.getByTestId("pilot-banner")).toContainText(
    "Essentials+ Merchant - Amazon Intelligence Pilot - Read-only",
  );
  await expect(page.getByTestId("pilot-banner")).toContainText("Fail-closed Pilotprofil aktiv");

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
        prompt_version: "mantle-amazon-weekly-strategy-v2",
        response_storage: "store_false",
        input_boundary: "aggregate_history_and_previous_handover_only",
        cadence: "manual_weekly",
        calendar_timezone: "Europe/Berlin",
        automatic_execution: false,
        mutation_capability: false,
      },
      can_run: true,
      block_reason: null,
      week_start: "2026-08-17",
      next_available_at: "2026-08-23T22:00:00Z",
      source_analysis_count: 2,
      previous_run_context: true,
      provider_request_id_redacted: null,
      input_tokens: null,
      output_tokens: null,
      assessment_week_start: null,
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
  await expect(strategyPanel).toContainText(aggregateHash);
  const strategyButton = strategyPanel.getByRole("button", { name: "Analyse", exact: true });
  await strategyButton.click();
  await expect(strategyPanel).toContainText("b".repeat(64));
  await expect(strategyButton).toBeEnabled();
  await strategyButton.click();
  await expect(strategyButton).toBeDisabled();
  await expect(strategyPanel.getByText("KI-generiert – keine Faktenquelle")).toBeVisible();
  await expect(strategyPanel).toContainText("Synthetische Chance");
  await expect(strategyPanel).toContainText("Hypothesen – nicht als Fakten behandeln");
  await expect(strategyPanel).toContainText("Handover bis zum nächsten Wochenlauf");
  await expect(strategyPanel).toContainText("Wochenlimit aktiv");
  await expect(manualAnalysis.locator(".analysis-block").first()).not.toContainText("Synthetische Chance");

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
  await expect(page.getByText("Interne Strategiehilfe für Mantle")).toBeVisible();
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
  await expect(page.getByRole("heading", { name: "Amazon AI Marketing" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Zugänge" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Abmelden" })).toHaveCount(0);

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
