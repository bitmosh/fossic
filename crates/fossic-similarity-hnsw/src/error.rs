// SPDX-License-Identifier: Apache-2.0
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HnswError {
    #[error("embedding dimension {got} does not match index dimension {expected}")]
    InvalidDimensions { expected: usize, got: usize },

    #[error("index file is corrupted or incompatible: {0}")]
    IndexCorrupted(String),

    #[error("mappings file version {0:#04x} is not supported (current: 0x01)")]
    MappingsVersionMismatch(u8),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("msgpack encode error: {0}")]
    MsgpackEncode(#[from] rmp_serde::encode::Error),

    #[error("msgpack decode error: {0}")]
    MsgpackDecode(#[from] rmp_serde::decode::Error),

    #[error("hnsw_rs error: {0}")]
    Hnsw(String),
}

impl From<HnswError> for fossic::Error {
    fn from(e: HnswError) -> Self {
        fossic::Error::Internal(e.to_string())
    }
}
