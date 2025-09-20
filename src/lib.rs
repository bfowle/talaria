// Talaria - High-performance FASTA sequence database reduction
// Global clippy configuration

#![warn(clippy::all)]
#![warn(clippy::correctness)]
#![warn(clippy::suspicious)]
#![warn(clippy::complexity)]
#![warn(clippy::perf)]
#![warn(clippy::style)]

// Allow some pedantic lints that don't add value
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::empty_line_after_doc_comments)]
#![allow(clippy::excessive_nesting)]

// Style preferences
#![allow(clippy::enum_glob_use)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::single_match_else)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::type_complexity)]

// Allow some common patterns
#![allow(clippy::len_zero)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::manual_strip)]
#![allow(clippy::single_char_add_str)]
#![allow(clippy::nonminimal_bool)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::inherent_to_string)]
#![allow(clippy::implicit_saturating_add)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::new_without_default)]
#![allow(clippy::manual_retain)]
#![allow(clippy::useless_format)]
#![allow(clippy::useless_vec)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::map_flatten)]
#![allow(clippy::single_match)]
#![allow(clippy::match_single_binding)]
#![allow(clippy::unnecessary_to_owned)]
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::let_and_return)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::from_over_into)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::option_map_unit_fn)]
#![allow(clippy::blocks_in_conditions)]
#![allow(clippy::get_first)]
#![allow(clippy::comparison_to_empty)]

pub mod bio;
pub mod casg;
pub mod cli;
pub mod core;
pub mod download;
pub mod index;
pub mod processing;
pub mod report;
pub mod storage;
pub mod tools;
pub mod utils;

pub use crate::core::{reducer::Reducer, reference_selector::ReferenceSelectorImpl};

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
