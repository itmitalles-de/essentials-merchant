import type { TranslationKey } from "./i18n/translations";
import type { InvoiceStatus } from "./types";

const KEYS: Record<InvoiceStatus, TranslationKey> = {
  draft: "invoices.status.draft",
  sent: "invoices.status.sent",
  overdue: "invoices.status.overdue",
  paid: "invoices.status.paid",
  cancelled: "invoices.status.cancelled",
};

export function invoiceStatusLabel(t: (key: TranslationKey) => string, status: InvoiceStatus): string {
  return t(KEYS[status]);
}
