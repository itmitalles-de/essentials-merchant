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

test("administrator completes the synthetic read-only Amazon pilot flow", async ({ page }) => {
  await page.goto("/login");
  await page.getByLabel("Language").selectOption("de");
  await page.getByLabel("Benutzername").fill(process.env.CORE_ADMIN_USERNAME ?? "admin");
  await page.getByLabel("Passwort").fill(process.env.CORE_ADMIN_PASSWORD ?? "ci-placeholder");
  await page.getByRole("button", { name: "Anmelden" }).click();

  await expect(page.getByTestId("pilot-banner")).toContainText(
    "Essentials+ Merchant - Amazon Intelligence Pilot - Read-only",
  );
  await expect(page.getByTestId("pilot-banner")).toContainText("Fail-closed Pilotprofil aktiv");

  await page.getByRole("link", { name: "Admin-Center" }).click();
  await expect(page.getByTestId("pilot-module-status")).toContainText("marketplace.amazon_intelligence");
  await expect(page.getByTestId("pilot-module-status")).toContainText("payment.test");
  await expect(page.getByTestId("pilot-module-status")).toContainText("export.datev");

  await page.getByRole("link", { name: "Marketplace Intelligence" }).click();
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

  await expect(page.locator("body")).not.toContainText(/refresh_token|client_secret|access_token/i);
  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(
    accessibility.violations.filter((violation) =>
      violation.impact === "serious" || violation.impact === "critical"),
  ).toEqual([]);
});
