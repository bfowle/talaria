/// Reference selection module

pub mod traits;
pub mod impls;

pub use traits::{
    ReferenceSelector, AlignmentBasedSelector, TaxonomyAwareSelector,
    ClusteringSelector, IncrementalSelector, SelectionResult,
    SelectionStats, AlignmentScore, RecommendedParams,
    SelectionUpdate, SelectionState, SelectorConfig,
};

pub use impls::{
    create_selector,
    create_configured_selector,
};