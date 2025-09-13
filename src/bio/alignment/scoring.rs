/// Scoring matrices for sequence alignment

use std::collections::HashMap;

pub trait ScoringMatrix {
    fn score(&self, a: u8, b: u8) -> i32;
    fn gap_open(&self) -> i32;
    fn gap_extend(&self) -> i32;
}

/// BLOSUM62 scoring matrix for protein sequences
pub struct BLOSUM62 {
    matrix: [[i32; 24]; 24],
    index_map: HashMap<u8, usize>,
    gap_open: i32,
    gap_extend: i32,
}

impl BLOSUM62 {
    pub fn new() -> Self {
        let mut index_map = HashMap::new();
        let amino_acids = b"ARNDCQEGHILKMFPSTWYVBZX*";
        for (i, &aa) in amino_acids.iter().enumerate() {
            index_map.insert(aa, i);
        }
        
        // BLOSUM62 matrix values (same as in original C++ code)
        let matrix = [
            [ 4, -1, -2, -2,  0, -1, -1,  0, -2, -1, -1, -1, -1, -2, -1,  1,  0, -3, -2,  0, -2, -1,  0, -4],
            [-1,  5,  0, -2, -3,  1,  0, -2,  0, -3, -2,  2, -1, -3, -2, -1, -1, -3, -2, -3, -1,  0, -1, -4],
            [-2,  0,  6,  1, -3,  0,  0,  0,  1, -3, -3,  0, -2, -3, -2,  1,  0, -4, -2, -3,  3,  0, -1, -4],
            [-2, -2,  1,  6, -3,  0,  2, -1, -1, -3, -4, -1, -3, -3, -1,  0, -1, -4, -3, -3,  4,  1, -1, -4],
            [ 0, -3, -3, -3,  9, -3, -4, -3, -3, -1, -1, -3, -1, -2, -3, -1, -1, -2, -2, -1, -3, -3, -2, -4],
            [-1,  1,  0,  0, -3,  5,  2, -2,  0, -3, -2,  1,  0, -3, -1,  0, -1, -2, -1, -2,  0,  3, -1, -4],
            [-1,  0,  0,  2, -4,  2,  5, -2,  0, -3, -3,  1, -2, -3, -1,  0, -1, -3, -2, -2,  1,  4, -1, -4],
            [ 0, -2,  0, -1, -3, -2, -2,  6, -2, -4, -4, -2, -3, -3, -2,  0, -2, -2, -3, -3, -1, -2, -1, -4],
            [-2,  0,  1, -1, -3,  0,  0, -2,  8, -3, -3, -1, -2, -1, -2, -1, -2, -2,  2, -3,  0,  0, -1, -4],
            [-1, -3, -3, -3, -1, -3, -3, -4, -3,  4,  2, -3,  1,  0, -3, -2, -1, -3, -1,  3, -3, -3, -1, -4],
            [-1, -2, -3, -4, -1, -2, -3, -4, -3,  2,  4, -2,  2,  0, -3, -2, -1, -2, -1,  1, -4, -3, -1, -4],
            [-1,  2,  0, -1, -3,  1,  1, -2, -1, -3, -2,  5, -1, -3, -1,  0, -1, -3, -2, -2,  0,  1, -1, -4],
            [-1, -1, -2, -3, -1,  0, -2, -3, -2,  1,  2, -1,  5,  0, -2, -1, -1, -1, -1,  1, -3, -1, -1, -4],
            [-2, -3, -3, -3, -2, -3, -3, -3, -1,  0,  0, -3,  0,  6, -4, -2, -2,  1,  3, -1, -3, -3, -1, -4],
            [-1, -2, -2, -1, -3, -1, -1, -2, -2, -3, -3, -1, -2, -4,  7, -1, -1, -4, -3, -2, -2, -1, -2, -4],
            [ 1, -1,  1,  0, -1,  0,  0,  0, -1, -2, -2,  0, -1, -2, -1,  4,  1, -3, -2, -2,  0,  0,  0, -4],
            [ 0, -1,  0, -1, -1, -1, -1, -2, -2, -1, -1, -1, -1, -2, -1,  1,  5, -2, -2,  0, -1, -1,  0, -4],
            [-3, -3, -4, -4, -2, -2, -3, -2, -2, -3, -2, -3, -1,  1, -4, -3, -2, 11,  2, -3, -4, -3, -2, -4],
            [-2, -2, -2, -3, -2, -1, -2, -3,  2, -1, -1, -2, -1,  3, -3, -2, -2,  2,  7, -1, -3, -2, -1, -4],
            [ 0, -3, -3, -3, -1, -2, -2, -3, -3,  3,  1, -2,  1, -1, -2, -2,  0, -3, -1,  4, -3, -2, -1, -4],
            [-2, -1,  3,  4, -3,  0,  1, -1,  0, -3, -4,  0, -3, -3, -2,  0, -1, -4, -3, -3,  4,  1, -1, -4],
            [-1,  0,  0,  1, -3,  3,  4, -2,  0, -3, -3,  1, -1, -3, -1,  0, -1, -3, -2, -2,  1,  4, -1, -4],
            [ 0, -1, -1, -1, -2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -2,  0,  0, -2, -1, -1, -1, -1, -1, -4],
            [-4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4,  1]
        ];
        
        Self {
            matrix,
            index_map,
            gap_open: 20,
            gap_extend: 10,
        }
    }
    
    pub fn with_gap_penalties(mut self, gap_open: i32, gap_extend: i32) -> Self {
        self.gap_open = gap_open;
        self.gap_extend = gap_extend;
        self
    }
}

impl Default for BLOSUM62 {
    fn default() -> Self {
        Self::new()
    }
}

impl ScoringMatrix for BLOSUM62 {
    fn score(&self, a: u8, b: u8) -> i32 {
        let a = a.to_ascii_uppercase();
        let b = b.to_ascii_uppercase();
        
        let i = self.index_map.get(&a).copied().unwrap_or(22); // X for unknown
        let j = self.index_map.get(&b).copied().unwrap_or(22);
        
        self.matrix[i][j]
    }
    
    fn gap_open(&self) -> i32 {
        self.gap_open
    }
    
    fn gap_extend(&self) -> i32 {
        self.gap_extend
    }
}

/// Nucleotide scoring matrix
pub struct NucleotideMatrix {
    match_score: i32,
    transition_score: i32,   // purine-purine or pyrimidine-pyrimidine
    transversion_score: i32, // purine-pyrimidine
    gap_open: i32,
    gap_extend: i32,
}

impl NucleotideMatrix {
    pub fn new() -> Self {
        Self {
            match_score: 10,
            transition_score: -5,
            transversion_score: -5,
            gap_open: 20,
            gap_extend: 10,
        }
    }
    
    pub fn with_scores(mut self, match_score: i32, mismatch_score: i32) -> Self {
        self.match_score = match_score;
        self.transition_score = mismatch_score;
        self.transversion_score = mismatch_score;
        self
    }
    
    pub fn with_gap_penalties(mut self, gap_open: i32, gap_extend: i32) -> Self {
        self.gap_open = gap_open;
        self.gap_extend = gap_extend;
        self
    }
    
    fn is_purine(base: u8) -> bool {
        matches!(base.to_ascii_uppercase(), b'A' | b'G')
    }
    
    fn is_pyrimidine(base: u8) -> bool {
        matches!(base.to_ascii_uppercase(), b'C' | b'T' | b'U')
    }
}

impl Default for NucleotideMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl ScoringMatrix for NucleotideMatrix {
    fn score(&self, a: u8, b: u8) -> i32 {
        let a = a.to_ascii_uppercase();
        let b = b.to_ascii_uppercase();
        
        if a == b {
            self.match_score
        } else if (Self::is_purine(a) && Self::is_purine(b)) ||
                  (Self::is_pyrimidine(a) && Self::is_pyrimidine(b)) {
            self.transition_score
        } else {
            self.transversion_score
        }
    }
    
    fn gap_open(&self) -> i32 {
        self.gap_open
    }
    
    fn gap_extend(&self) -> i32 {
        self.gap_extend
    }
}
