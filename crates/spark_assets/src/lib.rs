//! spark_assets — what comes in from disk.
//!
//! Importers, ours: a glTF 2.0 reader (`.glb` and `.gltf`) on top of our
//! own JSON parser, producing meshes in Spark's frame ready for the GPU.
//! Images ride along as the encoded bytes they were stored as — decoding
//! is FFmpeg's job, at the renderer's convenience.

pub mod glb;
pub mod gltf;
pub mod json;

pub use gltf::{Bounds, Image, Material, Model, Primitive, load};

/// Everything that can go wrong reading an asset.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Json(json::JsonError),
    /// The file is malformed: something the spec requires is missing or
    /// out of range.
    Invalid(String),
    /// The file is fine, and uses something Spark doesn't read yet.
    Unsupported(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Json(e) => write!(f, "JSON: {e}"),
            Error::Invalid(s) => write!(f, "invalid glTF: {s}"),
            Error::Unsupported(s) => write!(f, "unsupported glTF: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<json::JsonError> for Error {
    fn from(e: json::JsonError) -> Self {
        Error::Json(e)
    }
}

pub(crate) fn invalid(what: impl Into<String>) -> Error {
    Error::Invalid(what.into())
}
