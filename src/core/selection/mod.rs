/// Reference selection module
pub mod impls;
pub mod traits;

pub use traits::{
    AlignmentBasedSelector, AlignmentScore, RecommendedParams, ReferenceSelector, SelectionStats,
    TraitSelectionResult,
};

pub use impls::{create_configured_selector, create_selector};
