pub mod bio;
pub mod cli;
pub mod core;
pub mod download;
pub mod index;
pub mod report;
pub mod storage;
pub mod utils;

pub use crate::core::{reducer::Reducer, reference_selector::ReferenceSelector};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum TalariaError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Alignment error: {0}")]
    Alignment(String),
    
    #[error("Invalid configuration: {0}")]
    Config(String),
    
    #[error("Database error: {0}")]
    Database(String),
    
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, TalariaError>;