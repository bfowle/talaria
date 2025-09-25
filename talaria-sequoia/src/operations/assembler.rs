use talaria_bio::sequence::Sequence;
use talaria_bio::taxonomy::{TaxonomyFormatter, StandardTaxonomyFormatter};
use crate::storage::SEQUOIAStorage;
/// FASTA reassembly from content-addressed chunks
use crate::types::*;
use crate::verification::Verifier as SEQUOIAVerifier;
use anyhow::{Context, Result};
use std::io::Write;

pub struct FastaAssembler<'a> {
    storage: &'a SEQUOIAStorage,
    verify_on_assembly: bool,
    taxonomy_formatter: StandardTaxonomyFormatter,
}

impl<'a> FastaAssembler<'a> {
    pub fn new(storage: &'a SEQUOIAStorage) -> Self {
        Self {
            storage,
            verify_on_assembly: true,
            taxonomy_formatter: StandardTaxonomyFormatter,
        }
    }

    pub fn with_verification(mut self, verify: bool) -> Self {
        self.verify_on_assembly = verify;
        self
    }

    /// Assemble sequences from a list of chunk hashes
    pub fn assemble_from_chunks(&self, chunk_hashes: &[SHA256Hash]) -> Result<Vec<Sequence>> {
        let mut sequences = Vec::new();

        for hash in chunk_hashes {
            let chunk_sequences = self.extract_sequences_from_chunk(hash)?;
            sequences.extend(chunk_sequences);
        }

        Ok(sequences)
    }

    /// Stream assembly to a writer (memory-efficient)
    pub fn stream_assembly<W: Write>(
        &self,
        chunk_hashes: &[SHA256Hash],
        writer: &mut W,
    ) -> Result<usize> {
        let mut total_sequences = 0;

        for hash in chunk_hashes {
            let count = self.stream_chunk_to_writer(hash, writer)?;
            total_sequences += count;
        }

        // Ensure all data is written
        writer.flush()?;

        Ok(total_sequences)
    }

    /// Extract sequences from a single chunk
    fn extract_sequences_from_chunk(&self, hash: &SHA256Hash) -> Result<Vec<Sequence>> {
        // Get chunk data
        let chunk_data = self
            .storage
            .get_chunk(hash)
            .with_context(|| format!("Failed to retrieve chunk {}", hash))?;

        // Try to deserialize as ChunkManifest (new format)
        if let Ok(manifest) = serde_json::from_slice::<crate::ChunkManifest>(&chunk_data) {
            // Load actual sequences from canonical storage
            let mut sequences = Vec::new();

            for seq_hash in &manifest.sequence_refs {
                // Load canonical sequence from sequence storage
                let canonical = self.storage.sequence_storage
                    .load_canonical(seq_hash)
                    .with_context(|| format!("Failed to load canonical sequence {}", seq_hash))?;

                // Load representations to get accession and description
                let representations = self.storage.sequence_storage
                    .load_representations(seq_hash)
                    .unwrap_or_else(|_| crate::SequenceRepresentations {
                        canonical_hash: seq_hash.clone(),
                        representations: Vec::new(),
                    });

                // Use first representation for accession/description, or use hash as fallback
                let (id, description, taxon_id) = if let Some(first_repr) = representations.representations.first() {
                    let id = first_repr.accessions.first()
                        .cloned()
                        .unwrap_or_else(|| seq_hash.to_hex()[..8].to_string());
                    (id, first_repr.description.clone(), first_repr.taxon_id)
                } else {
                    // No representations, use hash prefix as ID
                    (seq_hash.to_hex()[..8].to_string(), None, None)
                };

                // Convert to bio sequence
                let mut seq = Sequence {
                    id,
                    description,
                    sequence: canonical.sequence,
                    taxon_id: taxon_id.map(|t| t.0),
                    taxonomy_sources: Default::default(),
                };

                // Override with chunk-level taxonomy if more specific
                if manifest.taxon_ids.len() == 1 && seq.taxon_id.is_none() {
                    seq.taxon_id = Some(manifest.taxon_ids[0].0);
                }

                sequences.push(seq);
            }

            return Ok(sequences);
        }

        // Try to deserialize as ChunkManifest (new format)
        if let Ok(manifest) = serde_json::from_slice::<crate::ChunkManifest>(&chunk_data) {
            // Load actual sequences from canonical storage
            let mut sequences = Vec::new();
            for seq_hash in &manifest.sequence_refs {
                if let Ok(canonical) = self.storage.sequence_storage.load_canonical(seq_hash) {
                    // Convert canonical to bio sequence
                    sequences.push(talaria_bio::sequence::Sequence {
                        id: seq_hash.to_hex(),
                        description: None,
                        sequence: canonical.sequence,
                        taxon_id: None,
                        taxonomy_sources: Default::default(),
                    });
                }
            }
            return Ok(sequences);
        }

        // Otherwise verify and parse as FASTA
        if self.verify_on_assembly {
            let actual_hash = SHA256Hash::compute(&chunk_data);
            if &actual_hash != hash {
                return Err(anyhow::anyhow!(
                    "Chunk verification failed: expected {}, got {}",
                    hash,
                    actual_hash
                ));
            }
        }

        // Parse FASTA from chunk
        self.parse_fasta(&chunk_data)
    }

    /// Stream a chunk directly to writer
    fn stream_chunk_to_writer<W: Write>(&self, hash: &SHA256Hash, writer: &mut W) -> Result<usize> {
        // Extract sequences (this handles both ChunkManifest and old formats)
        let sequences = self.extract_sequences_from_chunk(hash)?;

        // Write sequences as FASTA with TaxID in header if available
        for seq in &sequences {
            let header = self.taxonomy_formatter.format_header_with_taxid(
                &seq.id,
                seq.description.as_deref(),
                seq.taxon_id,
            );
            writeln!(writer, "{}", header)?;
            writer.write_all(&seq.sequence)?;
            writeln!(writer)?;
        }

        Ok(sequences.len())
    }

    /// Parse FASTA format from bytes
    fn parse_fasta(&self, data: &[u8]) -> Result<Vec<Sequence>> {
        let mut sequences = Vec::new();
        let mut current_id = String::new();
        let mut current_desc = String::new();
        let mut current_seq = Vec::new();
        let mut in_sequence = false;

        for line in data.split(|&b| b == b'\n') {
            if line.is_empty() {
                continue;
            }

            if line.starts_with(b">") {
                // Save previous sequence if any
                if in_sequence && !current_id.is_empty() {
                    sequences.push(Sequence {
                        id: current_id.clone(),
                        description: if current_desc.is_empty() {
                            None
                        } else {
                            Some(current_desc.clone())
                        },
                        sequence: current_seq.clone(),
                        taxon_id: self.extract_taxon_from_description(&current_desc),
                        taxonomy_sources: Default::default(),
                    });
                }

                // Parse new header
                let header = String::from_utf8_lossy(&line[1..]);
                let parts: Vec<&str> = header.splitn(2, ' ').collect();

                current_id = parts[0].to_string();
                current_desc = parts.get(1).unwrap_or(&"").to_string();
                current_seq.clear();
                in_sequence = true;
            } else if in_sequence {
                // Append to sequence
                current_seq.extend_from_slice(line);
            }
        }

        // Save last sequence
        if in_sequence && !current_id.is_empty() {
            sequences.push(Sequence {
                id: current_id,
                description: if current_desc.is_empty() {
                    None
                } else {
                    Some(current_desc)
                },
                sequence: current_seq,
                taxon_id: None,
                taxonomy_sources: Default::default(),
            });
        }

        Ok(sequences)
    }

    fn extract_taxon_from_description(&self, desc: &str) -> Option<u32> {
        // Look for TaxID= pattern
        if let Some(pos) = desc.find("TaxID=") {
            let start = pos + 6;
            let end = desc[start..]
                .find(|c: char| !c.is_numeric())
                .map(|i| start + i)
                .unwrap_or(desc.len());

            desc[start..end].parse().ok()
        } else {
            None
        }
    }

    /// Assemble with cryptographic proof verification
    pub fn assemble_with_proof(
        &self,
        chunk_hashes: &[SHA256Hash],
        manifest: &TemporalManifest,
    ) -> Result<AssemblyResult> {
        let verifier = SEQUOIAVerifier::new(self.storage, manifest);

        // Verify all chunks first
        let mut verification_errors = Vec::new();
        for hash in chunk_hashes {
            if let Err(e) = verifier.verify_chunk(hash) {
                verification_errors.push(VerificationError {
                    chunk_hash: hash.clone(),
                    error: e.to_string(),
                });
            }
        }

        if !verification_errors.is_empty() {
            return Ok(AssemblyResult {
                sequences: Vec::new(),
                chunks_assembled: 0,
                verification_errors,
                merkle_proof: None,
            });
        }

        // Assemble sequences
        let sequences = self.assemble_from_chunks(chunk_hashes)?;

        // Generate Merkle proof for assembled data
        let proof = verifier.generate_assembly_proof(chunk_hashes)?;

        Ok(AssemblyResult {
            sequences,
            chunks_assembled: chunk_hashes.len(),
            verification_errors,
            merkle_proof: Some(proof),
        })
    }

    /// Assemble a taxonomic subset
    pub fn assemble_taxonomic_subset(
        &self,
        taxon_ids: &[TaxonId],
        manifest: &TemporalManifest,
    ) -> Result<Vec<Sequence>> {
        // Find chunks containing these taxa
        let relevant_chunks: Vec<SHA256Hash> = manifest
            .chunk_index
            .iter()
            .filter(|chunk| chunk.taxon_ids.iter().any(|tid| taxon_ids.contains(tid)))
            .map(|chunk| chunk.hash.clone())
            .collect();

        // Assemble and filter
        let all_sequences = self.assemble_from_chunks(&relevant_chunks)?;

        // Filter to only requested taxa
        let filtered: Vec<Sequence> = all_sequences
            .into_iter()
            .filter(|seq| {
                seq.taxon_id
                    .map(|tid| taxon_ids.contains(&TaxonId(tid)))
                    .unwrap_or(false)
            })
            .collect();

        Ok(filtered)
    }

    /// Stream assembly with parallel chunk fetching
    pub async fn parallel_stream_assembly<W: Write + Send>(
        &self,
        chunk_hashes: &[SHA256Hash],
        writer: &mut W,
    ) -> Result<usize> {
        use futures::stream::{self, StreamExt};

        // Create futures with hash information preserved
        let chunk_futures: Vec<_> = chunk_hashes
            .iter()
            .map(|hash| {
                let hash_clone = hash.clone();
                async move {
                    let data = self.fetch_chunk_async(&hash_clone).await?;
                    Ok::<(SHA256Hash, Vec<u8>), anyhow::Error>((hash_clone, data))
                }
            })
            .collect();

        let mut chunk_stream = stream::iter(chunk_futures).buffer_unordered(4);

        let mut total_sequences = 0;

        while let Some(result) = chunk_stream.next().await {
            let (expected_hash, chunk_data) = result?;

            // Verify if requested
            if self.verify_on_assembly {
                let computed_hash = SHA256Hash::compute(&chunk_data);
                if computed_hash != expected_hash {
                    eprintln!(
                        "Warning: Chunk hash mismatch during assembly!\n  Expected: {}\n  Computed: {}",
                        expected_hash, computed_hash
                    );

                    if self.verify_on_assembly {
                        // In strict mode, fail on hash mismatch
                        return Err(anyhow::anyhow!(
                            "Chunk verification failed: hash mismatch for chunk {}",
                            expected_hash
                        ));
                    }
                }
            }

            // Write to output
            writer.write_all(&chunk_data)?;

            // Count sequences
            let sequences = self.parse_fasta(&chunk_data)?;
            total_sequences += sequences.len();
        }

        Ok(total_sequences)
    }

    async fn fetch_chunk_async(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
        // In a real implementation, this would be async storage access
        // For now, just return synchronously
        self.storage.get_chunk(hash)
    }
}

#[derive(Debug)]
pub struct AssemblyResult {
    pub sequences: Vec<Sequence>,
    pub chunks_assembled: usize,
    pub verification_errors: Vec<VerificationError>,
    pub merkle_proof: Option<MerkleProof>,
}

#[derive(Debug)]
pub struct VerificationError {
    pub chunk_hash: SHA256Hash,
    pub error: String,
}

/// Builder for complex assembly operations
pub struct AssemblyBuilder<'a> {
    assembler: FastaAssembler<'a>,
    chunk_hashes: Vec<SHA256Hash>,
    taxon_filter: Option<Vec<TaxonId>>,
    verify: bool,
    parallel: bool,
}

impl<'a> AssemblyBuilder<'a> {
    pub fn new(storage: &'a SEQUOIAStorage) -> Self {
        Self {
            assembler: FastaAssembler::new(storage),
            chunk_hashes: Vec::new(),
            taxon_filter: None,
            verify: true,
            parallel: false,
        }
    }

    pub fn add_chunk(mut self, hash: SHA256Hash) -> Self {
        self.chunk_hashes.push(hash);
        self
    }

    pub fn add_chunks(mut self, hashes: Vec<SHA256Hash>) -> Self {
        self.chunk_hashes.extend(hashes);
        self
    }

    pub fn filter_by_taxa(mut self, taxa: Vec<TaxonId>) -> Self {
        self.taxon_filter = Some(taxa);
        self
    }

    pub fn with_verification(mut self, verify: bool) -> Self {
        self.verify = verify;
        self.assembler = self.assembler.with_verification(verify);
        self
    }

    pub fn parallel(mut self) -> Self {
        self.parallel = true;
        self
    }

    pub fn build(self) -> Result<Vec<Sequence>> {
        let mut sequences = self.assembler.assemble_from_chunks(&self.chunk_hashes)?;

        // Apply taxon filter if specified
        if let Some(taxa) = self.taxon_filter {
            sequences.retain(|seq| {
                    seq.taxon_id
                        .map(|tid| taxa.contains(&TaxonId(tid)))
                        .unwrap_or(false)
                });
        }

        Ok(sequences)
    }

    pub fn stream_to<W: Write>(self, writer: &mut W) -> Result<usize> {
        self.assembler.stream_assembly(&self.chunk_hashes, writer)
    }
}
