//! Reusable, deterministic D2R extraction, normalization, and audit modules.

pub mod audit;
pub mod catalog;
pub mod error;
pub mod exporter;
pub mod model;
pub mod normalize;

pub use audit::{AuditReport, audit_snapshot, write_audit};
pub use catalog::{
    NAME_CATALOG_VERSION, NameCandidate, NameCatalog, catalog_local_install, normalize_catalog_name,
};
pub use error::{Error, Result};
pub use exporter::{
    ArchiveReader, SOURCE_WHITELIST, STATIC_MOD_TARGET_PATH, export_archive, export_with_reader,
    with_archive_reader,
};
pub use model::*;
pub use normalize::{normalize_input, normalize_to_path};
