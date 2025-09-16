/// Validation module

pub mod traits;

pub use traits::{
    Validator, SequenceValidator, ChunkValidator, FastaValidator, DeltaValidator,
    ValidationResult, ValidationError, ValidationWarning, ErrorLocation, ErrorSeverity,
    ValidationStats, ValidationRule, RuleCategory, StrictnessLevel,
    SequenceValidation, SequenceIssue, RepairResult, RepairChange,
    ChunkValidation, ConsistencyCheck, ReferenceValidation,
    FastaValidation, FormatError, HeaderValidation, FastaStats,
    DeltaValidation, ReconstructionValidation, EfficiencyReport,
};