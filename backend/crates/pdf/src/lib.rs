//! Typst-based invoice PDF rendering. Implemented in Phase 5.

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("not implemented yet")]
    NotImplemented,
}
