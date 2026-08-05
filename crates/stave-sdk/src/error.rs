use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StaveError {
    #[error("authentication: {0}")]
    Auth(String),

    #[error("operation '{0}' not found in the curated registry (see `stave ops list`)")]
    UnknownOperation(String),

    #[error("missing required parameter '{0}' for operation '{1}'")]
    MissingParam(String, String),

    #[error("invalid parameter '{0}': {1}")]
    InvalidParam(String, String),

    #[error(
        "write-guard: operation '{operation}' is a {op_type} and stave is \
         read-only by default against the live tenant. To proceed deliberately, pass \
         --allow-write, set STAVE_ALLOW_WRITE=1, or persist \
         `stave config set allow_writes true`."
    )]
    WriteGuard { operation: String, op_type: String },

    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    #[error("GraphQL: {}", messages.join("; "))]
    GraphQl { messages: Vec<String> },

    #[error("GraphQL document: {0}")]
    Document(String),

    #[error("network: {0}")]
    Network(String),

    #[error("spec: {0}")]
    Spec(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StaveError>;

impl From<reqwest::Error> for StaveError {
    fn from(e: reqwest::Error) -> Self {
        StaveError::Network(e.to_string())
    }
}
