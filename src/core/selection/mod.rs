/// Reference selection module

pub mod impls;
pub mod traits;

pub use traits::{
    ReferenceSelector, AlignmentBasedSelector,
    TraitSelectionResult, SelectionStats, AlignmentScore, RecommendedParams,
};

pub use impls::{
    create_selector,
    create_configured_selector,
};