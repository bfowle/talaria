/// Needleman-Wunsch global alignment algorithm
use crate::alignment::scoring::ScoringMatrix;
use crate::sequence::{Sequence, SequenceType};

#[derive(Debug, Clone)]
pub struct DetailedAlignment {
    pub score: i32,
    pub ref_aligned: Vec<u8>,
    pub query_aligned: Vec<u8>,
    pub alignment_string: Vec<u8>, // '|' for match, 'X' for mismatch, ' ' for gap
    pub deltas: Vec<Delta>,
    pub identity: f64, // Sequence identity (0.0 to 1.0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delta {
    pub position: usize,
    pub reference: u8,
    pub query: u8,
}

pub struct NeedlemanWunsch<S: ScoringMatrix> {
    scoring: S,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Traceback {
    Diagonal,
    Up,
    Left,
    None,
}

pub struct Alignment;

impl Alignment {
    pub fn global(ref_seq: &Sequence, query_seq: &Sequence) -> DetailedAlignment {
        if ref_seq.detect_type() == SequenceType::Protein {
            let aligner = NeedlemanWunsch::new(crate::alignment::scoring::BLOSUM62::new());
            aligner.align(&ref_seq.sequence, &query_seq.sequence)
        } else {
            let aligner =
                NeedlemanWunsch::new(crate::alignment::scoring::NucleotideMatrix::new());
            aligner.align(&ref_seq.sequence, &query_seq.sequence)
        }
    }
}

impl<S: ScoringMatrix> NeedlemanWunsch<S> {
    pub fn new(scoring: S) -> Self {
        Self { scoring }
    }

    pub fn align(&self, ref_seq: &[u8], query_seq: &[u8]) -> DetailedAlignment {
        let ref_len = ref_seq.len();
        let query_len = query_seq.len();

        // Allocate matrices
        let mut score_matrix = vec![vec![0i32; ref_len + 1]; query_len + 1];
        let mut traceback_matrix = vec![vec![Traceback::None; ref_len + 1]; query_len + 1];

        // Initialize matrices with gap penalties
        self.initialize_matrices(&mut score_matrix, &mut traceback_matrix, ref_len, query_len);

        // Fill matrices
        self.fill_matrices(&mut score_matrix, &mut traceback_matrix, ref_seq, query_seq);

        // Find optimal endpoint (for semi-global alignment)
        let (end_i, end_j) = self.find_optimal_endpoint(&score_matrix, query_len, ref_len);

        // Traceback to get alignment
        let (ref_aligned, query_aligned) =
            self.traceback(&traceback_matrix, ref_seq, query_seq, end_i, end_j);

        // Calculate alignment string and deltas
        let alignment_string = self.calculate_alignment_string(&ref_aligned, &query_aligned);
        let deltas = self.extract_deltas(&ref_aligned, &query_aligned);

        // Calculate identity as fraction of matching positions
        let matches = alignment_string.iter().filter(|&&c| c == b'|').count();
        let total_positions = alignment_string.len().max(1);
        let identity = matches as f64 / total_positions as f64;

        DetailedAlignment {
            score: score_matrix[end_i][end_j],
            ref_aligned,
            query_aligned,
            alignment_string,
            deltas,
            identity,
        }
    }

    fn initialize_matrices(
        &self,
        score_matrix: &mut Vec<Vec<i32>>,
        traceback_matrix: &mut Vec<Vec<Traceback>>,
        ref_len: usize,
        query_len: usize,
    ) {
        let gap_open = self.scoring.gap_open();
        let gap_extend = self.scoring.gap_extend();

        score_matrix[0][0] = 0;
        traceback_matrix[0][0] = Traceback::None;

        // Initialize first row
        for j in 1..=ref_len {
            score_matrix[0][j] = -(gap_open + gap_extend * (j as i32 - 1));
            traceback_matrix[0][j] = Traceback::Left;
        }

        // Initialize first column
        for i in 1..=query_len {
            score_matrix[i][0] = -(gap_open + gap_extend * (i as i32 - 1));
            traceback_matrix[i][0] = Traceback::Up;
        }
    }

    fn fill_matrices(
        &self,
        score_matrix: &mut Vec<Vec<i32>>,
        traceback_matrix: &mut Vec<Vec<Traceback>>,
        ref_seq: &[u8],
        query_seq: &[u8],
    ) {
        let gap_open = self.scoring.gap_open();
        let gap_extend = self.scoring.gap_extend();

        for i in 1..=query_seq.len() {
            for j in 1..=ref_seq.len() {
                // Calculate scores for each direction
                let match_score = self.scoring.score(ref_seq[j - 1], query_seq[i - 1]);
                let diagonal_score = score_matrix[i - 1][j - 1] + match_score;

                // Calculate gap penalties with affine model
                let mut up_gap_len = 1;
                let mut k = i - 1;
                while k > 0 && traceback_matrix[k][j] == Traceback::Up {
                    up_gap_len += 1;
                    k -= 1;
                }
                let up_score = score_matrix[i - 1][j] - (gap_open + gap_extend * (up_gap_len - 1));

                let mut left_gap_len = 1;
                let mut k = j - 1;
                while k > 0 && traceback_matrix[i][k] == Traceback::Left {
                    left_gap_len += 1;
                    k -= 1;
                }
                let left_score =
                    score_matrix[i][j - 1] - (gap_open + gap_extend * (left_gap_len - 1));

                // Choose best score
                let (best_score, direction) =
                    if diagonal_score >= up_score && diagonal_score >= left_score {
                        (diagonal_score, Traceback::Diagonal)
                    } else if up_score > left_score {
                        (up_score, Traceback::Up)
                    } else {
                        (left_score, Traceback::Left)
                    };

                score_matrix[i][j] = best_score;
                traceback_matrix[i][j] = direction;
            }
        }
    }

    fn find_optimal_endpoint(
        &self,
        score_matrix: &[Vec<i32>],
        query_len: usize,
        ref_len: usize,
    ) -> (usize, usize) {
        // For semi-global alignment, find the best score in the matrix
        let mut max_score = score_matrix[query_len][ref_len];
        let mut best_i = query_len;
        let mut best_j = ref_len;

        // Check last row (query fully aligned)
        for j in 0..=ref_len {
            if score_matrix[query_len][j] > max_score {
                max_score = score_matrix[query_len][j];
                best_i = query_len;
                best_j = j;
            }
        }

        // Check last column (reference fully aligned)
        for i in 0..=query_len {
            if score_matrix[i][ref_len] > max_score {
                max_score = score_matrix[i][ref_len];
                best_i = i;
                best_j = ref_len;
            }
        }

        (best_i, best_j)
    }

    fn traceback(
        &self,
        traceback_matrix: &[Vec<Traceback>],
        ref_seq: &[u8],
        query_seq: &[u8],
        end_i: usize,
        end_j: usize,
    ) -> (Vec<u8>, Vec<u8>) {
        let mut ref_aligned = Vec::new();
        let mut query_aligned = Vec::new();

        let mut i = end_i;
        let mut j = end_j;

        // Add trailing gaps if needed
        while j < ref_seq.len() {
            ref_aligned.push(ref_seq[j]);
            query_aligned.push(b'-');
            j += 1;
        }

        while i < query_seq.len() {
            ref_aligned.push(b'-');
            query_aligned.push(query_seq[i]);
            i += 1;
        }

        // Traceback from endpoint
        i = end_i;
        j = end_j;

        while i > 0 || j > 0 {
            match traceback_matrix[i][j] {
                Traceback::Diagonal => {
                    ref_aligned.push(ref_seq[j - 1]);
                    query_aligned.push(query_seq[i - 1]);
                    i -= 1;
                    j -= 1;
                }
                Traceback::Up => {
                    ref_aligned.push(b'-');
                    query_aligned.push(query_seq[i - 1]);
                    i -= 1;
                }
                Traceback::Left => {
                    ref_aligned.push(ref_seq[j - 1]);
                    query_aligned.push(b'-');
                    j -= 1;
                }
                Traceback::None => break,
            }
        }

        ref_aligned.reverse();
        query_aligned.reverse();

        (ref_aligned, query_aligned)
    }

    fn calculate_alignment_string(&self, ref_aligned: &[u8], query_aligned: &[u8]) -> Vec<u8> {
        ref_aligned
            .iter()
            .zip(query_aligned.iter())
            .map(|(&r, &q)| {
                if r == b'-' || q == b'-' {
                    b' '
                } else if r == q {
                    b'|'
                } else {
                    b'X'
                }
            })
            .collect()
    }

    fn extract_deltas(&self, ref_aligned: &[u8], query_aligned: &[u8]) -> Vec<Delta> {
        let mut deltas = Vec::new();
        let mut ref_pos = 0;

        for (&r, &q) in ref_aligned.iter().zip(query_aligned.iter()) {
            if r != b'-' {
                if q != b'-' && r != q {
                    deltas.push(Delta {
                        position: ref_pos,
                        reference: r,
                        query: q,
                    });
                }
                ref_pos += 1;
            }
        }

        deltas
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alignment::scoring::NucleotideMatrix;

    #[test]
    fn test_simple_alignment() {
        let aligner = NeedlemanWunsch::new(NucleotideMatrix::new());
        let ref_seq = b"ACGT";
        let query_seq = b"AGGT";

        let result = aligner.align(ref_seq, query_seq);

        assert_eq!(result.ref_aligned, b"ACGT");
        assert_eq!(result.query_aligned, b"AGGT");
        assert_eq!(result.deltas.len(), 1);
        assert_eq!(result.deltas[0].position, 1);
        assert_eq!(result.deltas[0].reference, b'C');
        assert_eq!(result.deltas[0].query, b'G');
    }

    #[test]
    fn test_alignment_with_gaps() {
        let aligner = NeedlemanWunsch::new(NucleotideMatrix::new());
        let ref_seq = b"ACGTACGT";
        let query_seq = b"ACGTCGT"; // Missing one 'A'

        let result = aligner.align(ref_seq, query_seq);

        // The alignment should have a positive score
        assert!(result.score > 0);
        // For this specific case with a deletion, deltas might be empty
        // since deltas only track substitutions, not indels
        // The alignment string should show the gap though
        assert_eq!(result.ref_aligned.len(), result.query_aligned.len());
    }
}
