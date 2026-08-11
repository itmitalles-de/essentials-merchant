//! Domain logic for ErpLite: VAT math, invoice/booking rules, DATEV formatting.
//! Deliberately free of DB/HTTP dependencies so it stays unit-testable in isolation.

pub mod invoice_status;
pub mod vat;

pub fn health() -> &'static str {
    "ok"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_reports_ok() {
        assert_eq!(health(), "ok");
    }
}
