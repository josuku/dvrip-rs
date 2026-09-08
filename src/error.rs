use thiserror::Error;

#[derive(Error, Debug)]
pub enum DVRIPError {
    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Not initialized")]
    NotInitialized(),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, DVRIPError>;
