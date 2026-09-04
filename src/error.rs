use std::path::PathBuf;

use busylib::types::invalid_value::InvalidValue;

pub type Result<T> = std::result::Result<T, CliError>;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("could not read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not write {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not convert {path}")]
    Convert {
        path: PathBuf,
        #[source]
        source: busybody::Error,
    },

    #[error(transparent)]
    Api(#[from] busylib::Error),

    #[error("the local API token has characters that cannot go in an HTTP header")]
    ApiToken(#[from] http::header::InvalidHeaderValue),
}

impl From<InvalidValue> for CliError {
    fn from(error: InvalidValue) -> Self {
        Self::Api(busylib::Error::Value(error))
    }
}
