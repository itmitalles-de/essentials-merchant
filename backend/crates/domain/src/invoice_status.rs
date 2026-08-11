/// Invoice lifecycle. `Overdue` is never set by a direct transition — it is
/// computed lazily on read (Phase 8) once a `Sent` invoice is past its due date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvoiceStatus {
    Draft,
    Sent,
    Overdue,
    Paid,
    Cancelled,
}

impl InvoiceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            InvoiceStatus::Draft => "draft",
            InvoiceStatus::Sent => "sent",
            InvoiceStatus::Overdue => "overdue",
            InvoiceStatus::Paid => "paid",
            InvoiceStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(InvoiceStatus::Draft),
            "sent" => Some(InvoiceStatus::Sent),
            "overdue" => Some(InvoiceStatus::Overdue),
            "paid" => Some(InvoiceStatus::Paid),
            "cancelled" => Some(InvoiceStatus::Cancelled),
            _ => None,
        }
    }

    /// Invoices become immutable (no line-item edits) once they leave draft.
    pub fn is_editable(self) -> bool {
        matches!(self, InvoiceStatus::Draft)
    }

    pub fn can_transition_to(self, target: InvoiceStatus) -> bool {
        use InvoiceStatus::*;
        matches!(
            (self, target),
            (Draft, Sent)
                | (Draft, Cancelled)
                | (Sent, Paid)
                | (Sent, Overdue)
                | (Sent, Cancelled)
                | (Overdue, Paid)
                | (Overdue, Cancelled)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::InvoiceStatus;
    use super::InvoiceStatus::*;

    #[test]
    fn draft_can_be_sent_or_cancelled_only() {
        assert!(Draft.can_transition_to(Sent));
        assert!(Draft.can_transition_to(Cancelled));
        assert!(!Draft.can_transition_to(Paid));
        assert!(!Draft.can_transition_to(Overdue));
        assert!(!Draft.can_transition_to(Draft));
    }

    #[test]
    fn sent_can_reach_paid_overdue_or_cancelled() {
        assert!(Sent.can_transition_to(Paid));
        assert!(Sent.can_transition_to(Overdue));
        assert!(Sent.can_transition_to(Cancelled));
        assert!(!Sent.can_transition_to(Draft));
    }

    #[test]
    fn overdue_can_reach_paid_or_cancelled() {
        assert!(Overdue.can_transition_to(Paid));
        assert!(Overdue.can_transition_to(Cancelled));
        assert!(!Overdue.can_transition_to(Sent));
    }

    #[test]
    fn paid_and_cancelled_are_terminal() {
        for target in [Draft, Sent, Overdue, Paid, Cancelled] {
            assert!(!Paid.can_transition_to(target));
            assert!(!Cancelled.can_transition_to(target));
        }
    }

    #[test]
    fn only_draft_is_editable() {
        assert!(Draft.is_editable());
        for status in [Sent, Overdue, Paid, Cancelled] {
            assert!(!status.is_editable());
        }
    }

    #[test]
    fn round_trips_through_str() {
        for status in [Draft, Sent, Overdue, Paid, Cancelled] {
            assert_eq!(InvoiceStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(InvoiceStatus::parse("unknown"), None);
    }
}
