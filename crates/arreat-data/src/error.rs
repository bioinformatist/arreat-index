use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid TSV in {path}: {source}")]
    Tsv {
        path: String,
        #[source]
        source: csv::Error,
    },
    #[error("archive entry is not a safe relative path: {0}")]
    UnsafePath(String),
    #[error("required input is missing: {0}")]
    MissingInput(String),
    #[error("unsupported platform: the CascLib exporter is available only on Linux")]
    UnsupportedPlatform,
    #[error("CascLib operation {operation} failed with error {code}")]
    Casc { operation: &'static str, code: u32 },
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Error {
    Error::Io {
        path: path.into(),
        source,
    }
}
