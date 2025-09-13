# Needleman-Wunsch Alignment

Talaria uses the Needleman-Wunsch algorithm for global sequence alignment to compute optimal alignments between reference and query sequences.

## Algorithm Overview

The Needleman-Wunsch algorithm is a dynamic programming approach that finds the optimal global alignment between two sequences by maximizing a similarity score.

## Mathematical Foundation

### Scoring Function

Given two sequences $S_1$ of length $m$ and $S_2$ of length $n$, we define a scoring function:

$$
\sigma(a, b) = \begin{cases}
    s_{match} & \text{if } a = b \\
    s_{mismatch} & \text{if } a \neq b \\
    s_{gap} & \text{if } a = - \text{ or } b = -
\end{cases}
$$

For proteins, we use the BLOSUM62 substitution matrix:

$$
\sigma(a, b) = \text{BLOSUM62}[a][b]
$$

### Dynamic Programming Matrix

We construct a matrix $F$ of size $(m+1) \times (n+1)$ where:

$$
F[i][j] = \text{score of optimal alignment of } S_1[1..i] \text{ with } S_2[1..j]
$$

### Initialization

$$
\begin{align}
F[0][0] &= 0 \\
F[i][0] &= i \cdot s_{gap} \quad \text{for } i = 1..m \\
F[0][j] &= j \cdot s_{gap} \quad \text{for } j = 1..n
\end{align}
$$

### Recurrence Relation

For $i = 1..m$ and $j = 1..n$:

$$
F[i][j] = \max \begin{cases}
    F[i-1][j-1] + \sigma(S_1[i], S_2[j]) & \text{(match/mismatch)} \\
    F[i-1][j] + s_{gap} & \text{(deletion)} \\
    F[i][j-1] + s_{gap} & \text{(insertion)}
\end{cases}
$$

### Optimal Score

The optimal alignment score is:

$$
\text{Score} = F[m][n]
$$

## Implementation Details

### Rust Implementation

```rust
pub struct NeedlemanWunsch<S: ScoringMatrix> {
    scoring_matrix: S,
    gap_penalty: i32,
}

impl<S: ScoringMatrix> NeedlemanWunsch<S> {
    pub fn align(&self, seq1: &[u8], seq2: &[u8]) -> AlignmentResult {
        let m = seq1.len();
        let n = seq2.len();
        
        // Initialize DP matrix
        let mut matrix = vec![vec![0i32; n + 1]; m + 1];
        
        // Initialization
        for i in 0..=m {
            matrix[i][0] = (i as i32) * self.gap_penalty;
        }
        for j in 0..=n {
            matrix[0][j] = (j as i32) * self.gap_penalty;
        }
        
        // Fill matrix
        for i in 1..=m {
            for j in 1..=n {
                let match_score = matrix[i-1][j-1] + 
                    self.scoring_matrix.score(seq1[i-1], seq2[j-1]);
                let delete_score = matrix[i-1][j] + self.gap_penalty;
                let insert_score = matrix[i][j-1] + self.gap_penalty;
                
                matrix[i][j] = match_score.max(delete_score).max(insert_score);
            }
        }
        
        // Traceback
        self.traceback(&matrix, seq1, seq2)
    }
}
```

### Time and Space Complexity

- **Time Complexity**: $O(m \times n)$
- **Space Complexity**: $O(m \times n)$
- **Space-Optimized**: $O(\min(m, n))$ for score only

### Memory Optimization

For large sequences, we use Hirschberg's algorithm which reduces space complexity:

$$
\text{Space} = O(m + n) \quad \text{instead of} \quad O(m \times n)
$$

## Scoring Matrices

### BLOSUM62 for Proteins

The BLOSUM62 matrix is based on observed substitution rates:

$$
\text{BLOSUM62}[i][j] = \frac{1}{2\lambda} \log_2 \left( \frac{q_{ij}}{e_i e_j} \right)
$$

Where:
- $q_{ij}$ = observed frequency of substitution
- $e_i, e_j$ = expected frequencies
- $\lambda$ = scaling factor

### DNA Scoring

For nucleotide sequences:

$$
\sigma(a, b) = \begin{cases}
    +2 & \text{if } a = b \\
    -1 & \text{if } a \neq b \\
    -2 & \text{for gaps}
\end{cases}
$$

## Affine Gap Penalties

For more realistic alignments, we use affine gap penalties:

$$
\text{Gap cost} = g_o + g_e \cdot l
$$

Where:
- $g_o$ = gap opening penalty
- $g_e$ = gap extension penalty
- $l$ = gap length

This requires three matrices:

$$
\begin{align}
M[i][j] &= \text{best score ending with match} \\
I_x[i][j] &= \text{best score ending with gap in } S_1 \\
I_y[i][j] &= \text{best score ending with gap in } S_2
\end{align}
$$

## Optimizations in Talaria

### 1. Banded Alignment

For similar sequences, we only compute a band around the diagonal:

$$
|i - j| \leq k
$$

This reduces complexity to $O(k \times \min(m, n))$.

### 2. SIMD Acceleration

We use SIMD instructions for parallel cell computation:

```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

unsafe fn compute_scores_simd(
    prev_row: &[i32],
    curr_row: &mut [i32],
    seq1_chunk: &[u8],
    seq2_byte: u8,
) {
    // Process 8 cells at once using AVX2
    let gap_penalty = _mm256_set1_epi32(GAP_PENALTY);
    // ... SIMD implementation
}
```

### 3. Cache-Efficient Access

We process the matrix in tiles to improve cache locality:

```rust
const TILE_SIZE: usize = 64;

for i_tile in (0..m).step_by(TILE_SIZE) {
    for j_tile in (0..n).step_by(TILE_SIZE) {
        process_tile(i_tile, j_tile, TILE_SIZE);
    }
}
```

## Quality Metrics

### Alignment Identity

$$
\text{Identity} = \frac{\text{Number of matches}}{\text{Alignment length}} \times 100\%
$$

### Normalized Score

$$
\text{Normalized Score} = \frac{S_{observed} - S_{random}}{S_{optimal} - S_{random}}
$$

Where:
- $S_{observed}$ = actual alignment score
- $S_{random}$ = expected score for random sequences
- $S_{optimal}$ = self-alignment score

### E-value Estimation

For database searches:

$$
E = K \cdot m \cdot n \cdot e^{-\lambda S}
$$

Where:
- $K, \lambda$ = Karlin-Altschul parameters
- $m, n$ = sequence and database lengths
- $S$ = alignment score

## Performance Characteristics

| Sequence Length | Time (ms) | Memory (MB) |
|----------------|-----------|-------------|
| 100 bp         | 0.1       | 0.04        |
| 1,000 bp       | 8         | 4           |
| 10,000 bp      | 800       | 400         |
| 100,000 bp     | 80,000    | 40,000      |

With banding (k=100):

| Sequence Length | Time (ms) | Memory (MB) |
|----------------|-----------|-------------|
| 100,000 bp     | 1,000     | 80          |
| 1,000,000 bp   | 10,000    | 800         |

## References

1. Needleman, S.B. and Wunsch, C.D. (1970). "A general method applicable to the search for similarities in the amino acid sequence of two proteins"
2. Hirschberg, D.S. (1975). "A linear space algorithm for computing maximal common subsequences"
3. Gotoh, O. (1982). "An improved algorithm for matching biological sequences"
