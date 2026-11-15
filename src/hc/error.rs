use serde_json::Value;
use thiserror::Error;

/// Error kinds returned by `herdr-core` (`data.kind` in JSON-RPC error responses)
/// as well as client-side transport/protocol errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HcErrorKind {
    // Core OpenRPC 1.3.2 data.kind values
    AlreadyAdded,
    Blocked,
    Cancelled,
    Config,
    Conflict,
    EditFailed,
    Finished,
    HcMissing,
    Incompatible,
    Internal,
    InvalidParams,
    InvalidTemplate,
    Io,
    Live,
    MachineUnknown,
    NameTaken,
    NeedsFramed,
    NeedsTarget,
    NeedsUpdate,
    NotFound,
    NotText,
    SpawnFailed,
    StatusMismatch,
    Unavailable,
    Unreachable,
    UnsupportedGeneration,
    Other(String),

    // Client-side transport/protocol kinds
    SocketUnreachable,
    MissingMandatoryMethods,
    ProtocolViolation,
    HeartbeatTimeout,
    ChannelClosed,
}

impl HcErrorKind {
    pub fn from_wire(kind: &str) -> Self {
        match kind {
            "already_added" => Self::AlreadyAdded,
            "blocked" => Self::Blocked,
            "cancelled" => Self::Cancelled,
            "config" => Self::Config,
            "conflict" => Self::Conflict,
            "edit_failed" => Self::EditFailed,
            "finished" => Self::Finished,
            "hc_missing" => Self::HcMissing,
            "incompatible" => Self::Incompatible,
            "internal" => Self::Internal,
            "invalid_params" => Self::InvalidParams,
            "invalid_template" => Self::InvalidTemplate,
            "io" => Self::Io,
            "live" => Self::Live,
            "machine_unknown" => Self::MachineUnknown,
            "name_taken" => Self::NameTaken,
            "needs_framed" => Self::NeedsFramed,
            "needs_target" => Self::NeedsTarget,
            "needs_update" => Self::NeedsUpdate,
            "not_found" => Self::NotFound,
            "not_text" => Self::NotText,
            "spawn_failed" => Self::SpawnFailed,
            "status_mismatch" => Self::StatusMismatch,
            "unavailable" => Self::Unavailable,
            "unreachable" => Self::Unreachable,
            "unsupported_generation" => Self::UnsupportedGeneration,
            other => Self::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Error)]
pub enum HcError {
    #[error("socket unreachable at {path}: {source}")]
    SocketUnreachable {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("RPC error ({kind:?}, code {code}): {message}")]
    Rpc {
        code: i64,
        message: String,
        kind: HcErrorKind,
        hint: Option<String>,
        data: Option<Value>,
    },

    #[error("core missing mandatory methods: {0:?}")]
    MissingMethods(Vec<String>),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("event stream heartbeat timeout after 45s")]
    HeartbeatTimeout,

    #[error("channel {0} closed")]
    ChannelClosed(u32),

    #[error("auto-start failed ({kind:?}): {message}")]
    AutoStartFailed { kind: HcErrorKind, message: String },
}

impl HcError {
    pub fn kind(&self) -> HcErrorKind {
        match self {
            Self::SocketUnreachable { .. } => HcErrorKind::SocketUnreachable,
            Self::Io(_) => HcErrorKind::Io,
            Self::Json(_) | Self::Protocol(_) => HcErrorKind::ProtocolViolation,
            Self::Rpc { kind, .. } => kind.clone(),
            Self::MissingMethods(_) => HcErrorKind::MissingMandatoryMethods,
            Self::HeartbeatTimeout => HcErrorKind::HeartbeatTimeout,
            Self::ChannelClosed(_) => HcErrorKind::ChannelClosed,
            Self::AutoStartFailed { kind, .. } => kind.clone(),
        }
    }

    pub fn from_rpc_error(code: i64, message: String, data: Option<Value>) -> Self {
        let kind_str = data
            .as_ref()
            .and_then(|d| d.get("kind"))
            .and_then(|k| k.as_str());
        let kind = kind_str
            .map(HcErrorKind::from_wire)
            .unwrap_or(HcErrorKind::Internal);
        let hint = data
            .as_ref()
            .and_then(|d| d.get("hint"))
            .and_then(|h| h.as_str())
            .map(String::from);
        Self::Rpc {
            code,
            message,
            kind,
            hint,
            data,
        }
    }
}

pub type HcResult<T> = Result<T, HcError>;
