use rust_decimal::{Decimal, RoundingStrategy};

/// Rounds to 2 decimal places using kaufmännisches Runden (round-half-away-from-zero),
/// the rounding convention German invoicing/accounting expects.
pub fn round_money(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Computes (vat_amount, gross_amount) for a net amount at a given VAT rate (as a
/// percentage, e.g. 19 for 19%). Both results are rounded to 2 decimal places.
pub fn calc_vat(net: Decimal, rate_percent: Decimal) -> (Decimal, Decimal) {
    let vat = round_money(net * rate_percent / Decimal::from(100));
    let gross = round_money(net + vat);
    (vat, gross)
}

/// One VAT-rate group in an invoice's breakdown table (§14 UStG requires this for
/// invoices mixing multiple rates): summed net/vat/gross for all line items at that rate.
#[derive(Debug, Clone, PartialEq)]
pub struct VatBreakdownRow {
    pub rate_percent: Decimal,
    pub net_total: Decimal,
    pub vat_total: Decimal,
    pub gross_total: Decimal,
}

/// Aggregates per-line-item (net, rate_percent) pairs into one row per distinct rate,
/// sorted by descending rate so the standard rate appears first.
pub fn vat_breakdown(line_items: &[(Decimal, Decimal)]) -> Vec<VatBreakdownRow> {
    let mut rows: Vec<VatBreakdownRow> = Vec::new();

    for &(net, rate_percent) in line_items {
        let (vat, gross) = calc_vat(net, rate_percent);
        match rows.iter_mut().find(|r| r.rate_percent == rate_percent) {
            Some(row) => {
                row.net_total += net;
                row.vat_total += vat;
                row.gross_total += gross;
            }
            None => rows.push(VatBreakdownRow {
                rate_percent,
                net_total: net,
                vat_total: vat,
                gross_total: gross,
            }),
        }
    }

    rows.sort_by_key(|row| std::cmp::Reverse(row.rate_percent));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn calc_vat_standard_rate() {
        let (vat, gross) = calc_vat(dec!(100.00), dec!(19));
        assert_eq!(vat, dec!(19.00));
        assert_eq!(gross, dec!(119.00));
    }

    #[test]
    fn calc_vat_reduced_rate() {
        let (vat, gross) = calc_vat(dec!(100.00), dec!(7));
        assert_eq!(vat, dec!(7.00));
        assert_eq!(gross, dec!(107.00));
    }

    #[test]
    fn calc_vat_zero_rate() {
        let (vat, gross) = calc_vat(dec!(100.00), dec!(0));
        assert_eq!(vat, dec!(0.00));
        assert_eq!(gross, dec!(100.00));
    }

    #[test]
    fn calc_vat_rounds_half_away_from_zero() {
        // 0.125 * 19% would be 0.02375 -> rounds to 0.02; pick a case that lands on .xx5
        let (vat, _) = calc_vat(dec!(0.50), dec!(19));
        // 0.50 * 0.19 = 0.095 -> rounds to 0.10 (away from zero, not banker's rounding)
        assert_eq!(vat, dec!(0.10));
    }

    #[test]
    fn vat_breakdown_groups_by_rate() {
        let items = vec![
            (dec!(100.00), dec!(19)),
            (dec!(50.00), dec!(19)),
            (dec!(200.00), dec!(7)),
        ];
        let rows = vat_breakdown(&items);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].rate_percent, dec!(19));
        assert_eq!(rows[0].net_total, dec!(150.00));
        assert_eq!(rows[0].vat_total, dec!(28.50));
        assert_eq!(rows[1].rate_percent, dec!(7));
        assert_eq!(rows[1].net_total, dec!(200.00));
        assert_eq!(rows[1].vat_total, dec!(14.00));
    }

    #[test]
    fn vat_breakdown_empty_for_no_items() {
        assert_eq!(vat_breakdown(&[]), vec![]);
    }
}
