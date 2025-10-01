use crate::sequence::Sequence;
use anyhow::Result;
use flate2::read::GzDecoder;
use memmap2::Mmap;
use nom::{
    bytes::complete::{tag, take_till},
    character::complete::{line_ending, not_line_ending},
    combinator::{map, opt},
    sequence::preceded,
    IResult,
};
use rayon::prelude::*;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use talaria_core::error::TalariaError;

/// Parse a FASTA header line
fn parse_header(input: &[u8]) -> IResult<&[u8], (&str, Option<&str>)> {
    let (input, _) = tag(b">")(input)?;
    let (input, id) = map(
        take_till(|c: u8| c == b' ' || c == b'\t' || c == b'\n' || c == b'\r'),
        |s| std::str::from_utf8(s).unwrap_or(""),
    )(input)?;
    let (input, description) = opt(preceded(
        tag(b" "),
        map(not_line_ending, |s| std::str::from_utf8(s).unwrap_or("")),
    ))(input)?;
    let (input, _) = line_ending(input)?;
    Ok((input, (id, description)))
}

/// Parse sequence lines until next header or EOF
fn parse_sequence(input: &[u8]) -> IResult<&[u8], (Vec<u8>, Option<String>)> {
    let mut sequence = Vec::new();
    let mut remaining = input;
    let mut continuation_lines = Vec::new();
    let mut line_num = 0;

    while !remaining.is_empty() && remaining[0] != b'>' {
        // Parse a line
        let (rest, line) =
            take_till::<_, _, nom::error::Error<_>>(|c: u8| c == b'\n' || c == b'\r')(remaining)?;
        let (rest, _) = opt(line_ending)(rest)?;

        let line_str = std::str::from_utf8(line).unwrap_or("");

        // First few lines might be wrapped header continuations
        // Look for UniProt-style metadata patterns
        if line_num < 3 && !line.is_empty() {
            // Check if line contains metadata patterns
            let has_metadata = line_str.contains("OX=")
                || line_str.contains("OS=")
                || line_str.contains("GN=")
                || line_str.contains("PE=")
                || line_str.contains("SV=");

            // Check if line starts with a digit followed by space (like "3 SV=")
            let starts_with_digit_space =
                line.len() >= 2 && line[0].is_ascii_digit() && line[1] == b' ';

            if has_metadata || starts_with_digit_space {
                // This looks like metadata continuation
                if starts_with_digit_space && line_str.contains("SV=") {
                    // Handle "3 SV=1SEQUENCE..." case
                    if let Some(sv_pos) = line_str.find("SV=") {
                        // Find where the version number ends (usually 1 digit after SV=)
                        let version_end = sv_pos + 3 + // "SV="
                            line_str[sv_pos + 3..]
                                .chars()
                                .take_while(|c| c.is_ascii_digit())
                                .count();

                        let metadata_part = &line_str[..version_end];
                        continuation_lines.push(metadata_part.to_string());

                        // The rest is sequence data
                        if version_end < line.len() {
                            for &c in &line[version_end..] {
                                if !c.is_ascii_whitespace() && c != b'=' {
                                    sequence.push(c.to_ascii_uppercase());
                                }
                            }
                        }
                    }
                } else {
                    // Full line is metadata
                    continuation_lines.push(line_str.to_string());
                }
                line_num += 1;
                remaining = rest;
                continue;
            }
        }

        // Regular sequence line
        for &c in line {
            if !c.is_ascii_whitespace() {
                sequence.push(c.to_ascii_uppercase());
            }
        }

        line_num += 1;
        remaining = rest;
    }

    let continuation_desc = if !continuation_lines.is_empty() {
        Some(continuation_lines.join(" "))
    } else {
        None
    };

    Ok((remaining, (sequence, continuation_desc)))
}

/// Parse a single FASTA record
fn parse_record(input: &[u8]) -> IResult<&[u8], Sequence> {
    let (input, (id, description)) = parse_header(input)?;
    let (input, (sequence, continuation_desc)) = parse_sequence(input)?;

    let mut seq = Sequence::new(id.to_string(), sequence);

    // Combine description with continuation if present
    let full_desc = match (description, continuation_desc.as_deref()) {
        (Some(desc), Some(cont)) => Some(format!("{} {}", desc, cont)),
        (Some(desc), None) => Some(desc.to_string()),
        (None, Some(cont)) => Some(cont.to_string()),
        (None, None) => None,
    };

    if let Some(desc) = full_desc {
        seq = seq.with_description(desc.clone());

        // Extract taxon ID from the full description
        if let Some(taxon) = extract_taxon_id(&desc) {
            // Only use non-zero taxon IDs
            if taxon > 0 {
                seq = seq.with_taxon(taxon);
            }
        }
    }

    Ok((input, seq))
}

/// Extract taxon ID from description (handles various formats)
pub fn extract_taxon_id(description: &str) -> Option<u32> {
    // First check for TaxID - if it's non-zero, use it
    if let Some(pos) = description.find("TaxID=") {
        let start = pos + "TaxID=".len();
        let taxon_str: String = description[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();

        if let Ok(taxon) = taxon_str.parse::<u32>() {
            if taxon > 0 {
                return Some(taxon);
            }
            // TaxID=0, fall through to check OX=
        }
    }

    // Try OX= (UniProt organism taxon) as priority fallback
    if let Some(pos) = description.find("OX=") {
        let start = pos + "OX=".len();
        let taxon_str: String = description[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();

        if let Ok(taxon) = taxon_str.parse::<u32>() {
            if taxon > 0 {
                return Some(taxon);
            }
        }
    }

    // Try other patterns
    let patterns = [("taxon:", ""), ("tax_id=", "")];

    for (prefix, _) in patterns {
        if let Some(pos) = description.find(prefix) {
            let start = pos + prefix.len();
            let taxon_str: String = description[start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();

            if let Ok(taxon) = taxon_str.parse::<u32>() {
                if taxon > 0 {
                    return Some(taxon);
                }
            }
        }
    }
    None
}

/// Parse FASTA from bytes
pub fn parse_fasta_from_bytes(data: &[u8]) -> Result<Vec<Sequence>, TalariaError> {
    let mut sequences = Vec::new();
    let mut remaining = data;

    while !remaining.is_empty() {
        // Skip empty lines and whitespace
        while !remaining.is_empty() && remaining[0].is_ascii_whitespace() {
            if let Ok((rest, _)) = line_ending::<_, nom::error::Error<_>>(remaining) {
                remaining = rest;
            } else {
                remaining = &remaining[1..];
            }
        }

        if remaining.is_empty() {
            break;
        }

        // Must start with '>'
        if remaining[0] != b'>' {
            break;
        }

        // Parse header
        let (rest, (id, description)) = parse_header(remaining)
            .map_err(|_| TalariaError::Parse("Failed to parse FASTA header".to_string()))?;

        // Check if the description contains embedded sequence (like "SV=1ACGTACGT")
        let mut embedded_seq = Vec::new();
        let final_description = if let Some(desc) = description {
            let desc_str = desc.to_string();
            // Look for patterns like SV=XSEQUENCE where X is a digit and SEQUENCE is uppercase letters
            if let Some(sv_pos) = desc_str.find("SV=") {
                let after_sv = &desc_str[sv_pos + 3..];
                // Find where digits end and potential sequence begins
                let digit_end = after_sv.chars().take_while(|c| c.is_ascii_digit()).count();
                if digit_end > 0 && digit_end < after_sv.len() {
                    let potential_seq = &after_sv[digit_end..];
                    // Check if the rest looks like a sequence (all uppercase letters)
                    if potential_seq.chars().all(|c| c.is_ascii_uppercase()) {
                        // Split the description and sequence
                        embedded_seq = potential_seq.as_bytes().to_vec();
                        // Keep the description up to and including SV=X
                        Some(desc_str[..sv_pos + 3 + digit_end].to_string())
                    } else {
                        Some(desc_str)
                    }
                } else {
                    Some(desc_str)
                }
            } else {
                Some(desc_str)
            }
        } else {
            None
        };

        // Parse sequence from next lines
        let (rest, (mut seq_data, continuation_desc)) = parse_sequence(rest)
            .map_err(|_| TalariaError::Parse("Failed to parse FASTA sequence".to_string()))?;

        // If we found embedded sequence in the header, prepend it
        if !embedded_seq.is_empty() {
            embedded_seq.extend(seq_data);
            seq_data = embedded_seq;
        }

        let mut seq = Sequence::new(id.to_string(), seq_data);

        // Combine description with continuation if present
        let full_desc = match (final_description.as_deref(), continuation_desc.as_deref()) {
            (Some(desc), Some(cont)) => Some(format!("{} {}", desc, cont)),
            (Some(desc), None) => Some(desc.to_string()),
            (None, Some(cont)) => Some(cont.to_string()),
            (None, None) => None,
        };

        if let Some(desc) = full_desc {
            seq.description = Some(desc.clone());

            // Extract taxon ID from the full description
            if let Some(taxon) = extract_taxon_id(&desc) {
                // Only use non-zero taxon IDs
                if taxon > 0 {
                    seq = seq.with_taxon(taxon);
                }
            }
        }
        sequences.push(seq);

        remaining = rest;
    }

    Ok(sequences)
}

/// Parse a FASTA file into sequences (supports .gz compression)
pub fn parse_fasta<P: AsRef<Path>>(path: P) -> Result<Vec<Sequence>, TalariaError> {
    let path = path.as_ref();

    // Check if file is gzipped
    if path.extension().and_then(|s| s.to_str()) == Some("gz") {
        parse_fasta_gzip(path)
    } else {
        parse_fasta_uncompressed(path)
    }
}

/// Parse an uncompressed FASTA file
fn parse_fasta_uncompressed(path: &Path) -> Result<Vec<Sequence>, TalariaError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };

    parse_fasta_buffer(&mmap[..])
}

/// Parse a gzipped FASTA file
fn parse_fasta_gzip(path: &Path) -> Result<Vec<Sequence>, TalariaError> {
    let file = File::open(path)?;
    let mut decoder = GzDecoder::new(BufReader::new(file));
    let mut buffer = Vec::new();
    decoder.read_to_end(&mut buffer)?;

    parse_fasta_buffer(&buffer)
}

/// Parse FASTA from a byte buffer
fn parse_fasta_buffer(buffer: &[u8]) -> Result<Vec<Sequence>, TalariaError> {
    let mut input = buffer;
    let mut sequences = Vec::new();

    while !input.is_empty() {
        // Skip empty lines and whitespace
        while !input.is_empty() && input[0].is_ascii_whitespace() {
            input = &input[1..];
        }

        if input.is_empty() {
            break;
        }

        match parse_record(input) {
            Ok((remaining, seq)) => {
                if !seq.is_empty() {
                    sequences.push(seq);
                }
                input = remaining;
            }
            Err(e) => {
                return Err(TalariaError::Parse(format!(
                    "Failed to parse FASTA: {:?}",
                    e
                )));
            }
        }
    }

    Ok(sequences)
}

/// Write sequences to a FASTA file (supports .gz compression)
pub fn write_fasta<P: AsRef<Path>>(path: P, sequences: &[Sequence]) -> Result<(), TalariaError> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let path = path.as_ref();
    let file = File::create(path)?;

    // Check if we should compress based on extension
    if path.extension().and_then(|s| s.to_str()) == Some("gz") {
        let encoder = GzEncoder::new(file, Compression::default());
        let mut writer = BufWriter::new(encoder);
        write_fasta_to_writer(&mut writer, sequences)?;
        writer.flush()?;
    } else {
        let mut writer = BufWriter::new(file);
        write_fasta_to_writer(&mut writer, sequences)?;
        writer.flush()?;
    }

    Ok(())
}

/// Write sequences to any writer with progress tracking
fn write_fasta_to_writer<W: Write>(
    writer: &mut W,
    sequences: &[Sequence],
) -> Result<(), TalariaError> {
    use indicatif::{ProgressBar, ProgressStyle};
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // Only show progress for large datasets
    let show_progress = sequences.len() > 1000;

    let pb = if show_progress && std::env::var("TALARIA_SILENT").is_err() {
        let pb = ProgressBar::new(sequences.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} sequences ({per_sec}, ETA: {eta})")
                .unwrap()
                .progress_chars("##-"),
        );
        pb.set_message("Formatting FASTA output...");
        Some(Arc::new(pb))
    } else {
        None
    };

    let processed = Arc::new(AtomicUsize::new(0));

    // Process sequences in parallel chunks
    let chunk_size = 1000; // Process 1000 sequences per chunk
    let chunks: Vec<Vec<u8>> = sequences
        .par_chunks(chunk_size)
        .map(|chunk_sequences| {
            // Estimate size for this chunk
            let chunk_estimated_size: usize = chunk_sequences
                .iter()
                .map(|s| {
                    s.id.len()
                        + s.description.as_ref().map_or(0, |d| d.len() + 1)
                        + s.sequence.len()
                        + (s.sequence.len() / 80)
                        + 10 // newlines and overhead
                })
                .sum();

            let mut chunk_buffer = Vec::with_capacity(chunk_estimated_size);

            for seq in chunk_sequences.iter() {
                // Write header
                chunk_buffer.extend_from_slice(seq.header().as_bytes());
                chunk_buffer.push(b'\n');

                // Write sequence in 80-character lines directly as bytes
                for seq_chunk in seq.sequence.chunks(80) {
                    chunk_buffer.extend_from_slice(seq_chunk);
                    chunk_buffer.push(b'\n');
                }

                // Update progress counter
                let count = processed.fetch_add(1, Ordering::Relaxed);
                if let Some(ref pb) = pb {
                    if count % 100 == 0 {
                        pb.set_position(count as u64);
                    }
                }
            }

            chunk_buffer
        })
        .collect();

    if let Some(ref pb) = pb {
        pb.set_position(sequences.len() as u64);
        pb.finish_with_message("Writing to disk...");
    }

    // Write all chunks sequentially
    for chunk in chunks {
        writer.write_all(&chunk)?;
    }

    Ok(())
}

/// Parse FASTA in parallel chunks for large files (supports .gz compression)
pub fn parse_fasta_parallel<P: AsRef<Path>>(
    path: P,
    chunk_size: usize,
) -> Result<Vec<Sequence>, TalariaError> {
    let path = path.as_ref();

    // For gzipped files, fall back to sequential parsing
    if path.extension().and_then(|s| s.to_str()) == Some("gz") {
        return parse_fasta(path);
    }

    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };

    // Find record boundaries for parallel processing
    let mut boundaries = vec![0];
    let mut pos = 0;

    while pos < mmap.len() {
        if pos > 0
            && mmap[pos] == b'>'
            && (pos == 0 || mmap[pos - 1] == b'\n')
            && boundaries
                .last()
                .is_none_or(|&last| pos - last >= chunk_size)
        {
            boundaries.push(pos);
        }
        pos += 1;
    }
    boundaries.push(mmap.len());

    // Parse chunks in parallel
    let sequences: Result<Vec<Vec<Sequence>>, TalariaError> = boundaries
        .par_windows(2)
        .map(|window| {
            let start = window[0];
            let end = window[1];
            let chunk = &mmap[start..end];

            let mut input = chunk;
            let mut chunk_sequences = Vec::new();

            while !input.is_empty() {
                while !input.is_empty() && input[0].is_ascii_whitespace() {
                    input = &input[1..];
                }

                if input.is_empty() {
                    break;
                }

                match parse_record(input) {
                    Ok((remaining, seq)) => {
                        if !seq.is_empty() {
                            chunk_sequences.push(seq);
                        }
                        input = remaining;
                    }
                    Err(e) => {
                        return Err(TalariaError::Parse(format!(
                            "Failed to parse FASTA chunk: {:?}",
                            e
                        )));
                    }
                }
            }

            Ok(chunk_sequences)
        })
        .collect();

    Ok(sequences?.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header() {
        let input = b">sp|P12345|PROTEIN_HUMAN Description here\nACGT";
        let (remaining, (id, desc)) = parse_header(input).unwrap();
        assert_eq!(id, "sp|P12345|PROTEIN_HUMAN");
        assert_eq!(desc, Some("Description here"));
        assert_eq!(remaining, b"ACGT");
    }

    #[test]
    fn test_extract_taxon_id() {
        assert_eq!(extract_taxon_id("Description OX=9606 GN=GENE"), Some(9606));
        assert_eq!(extract_taxon_id("TaxID=12345"), Some(12345));
        assert_eq!(extract_taxon_id("taxon:98765"), Some(98765));
        assert_eq!(extract_taxon_id("No taxon here"), None);
    }

    #[test]
    fn test_extract_taxon_id_with_zero_fallback() {
        // TaxID=0 should fall back to OX= field
        assert_eq!(extract_taxon_id("TaxID=0 OX=666"), Some(666));
        assert_eq!(
            extract_taxon_id("Something TaxID=0 more text OX=9606 end"),
            Some(9606)
        );

        // TaxID=0 with no OX= should return None (not 0)
        assert_eq!(extract_taxon_id("TaxID=0"), None);

        // Non-zero TaxID should be used even if OX= exists
        assert_eq!(extract_taxon_id("TaxID=123 OX=456"), Some(123));
    }

    #[test]
    fn test_extract_taxon_id_uniref_format() {
        // Test UniRef50 style headers
        assert_eq!(
            extract_taxon_id("UniRef50_A0A024RBG1 Cluster member n=1 Tax=Human TaxID=9606"),
            Some(9606)
        );
        assert_eq!(
            extract_taxon_id("UniRef50_Q8T6B1 Sodium channel TaxID=9606 RepID=Q8T6B1_HUMAN"),
            Some(9606)
        );
        assert_eq!(
            extract_taxon_id(
                "UniRef50_A0A0E3J5A9 Cluster: PREDICTED: mucin-5AC n=2 Tax=Equus TaxID=9796"
            ),
            Some(9796)
        );
        // Mixed formats - TaxID takes priority
        assert_eq!(
            extract_taxon_id("UniRef50_P12345 Some protein OX=12345 TaxID=67890"),
            Some(67890)
        );
        // Only OX= field
        assert_eq!(
            extract_taxon_id("UniRef50_P12345 Some protein OX=3702 RepID=P12345_ARATH"),
            Some(3702)
        );
    }

    #[test]
    fn test_parse_wrapped_header() {
        // Test wrapped header with continuation line
        let input = b">tr|A0A0H6|A0A0H6_VIBCL\nFatty acid oxidation OS=Vibrio OX=666\nMKLTF";
        let result = parse_fasta_from_bytes(input).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "tr|A0A0H6|A0A0H6_VIBCL");
        assert!(result[0]
            .description
            .as_ref()
            .unwrap()
            .contains("Fatty acid oxidation"));
        assert_eq!(result[0].taxon_id, Some(666));
        assert_eq!(result[0].sequence, b"MKLTF");
    }

    #[test]
    fn test_parse_metadata_bleeding() {
        // Test SV=1SEQUENCE format where metadata bleeds into sequence
        let input = b">test PE=1 SV=1ACGTACGT\n";
        let result = parse_fasta_from_bytes(input).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(
            String::from_utf8(result[0].sequence.clone()).unwrap(),
            "ACGTACGT"
        );
        assert!(result[0].description.as_ref().unwrap().contains("SV=1"));
    }

    #[test]
    fn test_parse_sequence_with_digit_space() {
        // Test "3 SV=1" format - parse_sequence expects input after the header
        let input = b"3 SV=1MKLTFFF\n";
        let (_, (sequence, continuation)) = parse_sequence(input).unwrap();

        assert!(continuation.is_some());
        assert!(continuation.as_ref().unwrap().contains("3 SV=1"));
        assert_eq!(sequence, b"MKLTFFF");
    }

    #[test]
    fn test_parse_multiple_records() {
        let input = b">seq1 OX=123\nACGT\n>seq2 TaxID=456\nTGCA\n>seq3\nAAAA\n";
        let result = parse_fasta_from_bytes(input).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id, "seq1");
        assert_eq!(result[0].taxon_id, Some(123));
        assert_eq!(result[1].id, "seq2");
        assert_eq!(result[1].taxon_id, Some(456));
        assert_eq!(result[2].id, "seq3");
        assert_eq!(result[2].taxon_id, None);
    }
}

/// Trait representing the capability to read FASTA files with automatic compression detection
/// This is a capability trait (adjective: "Readable"), not a data structure
pub trait FastaReadable {
    /// Open a FASTA file for reading, automatically detecting compression
    fn open_for_reading<P: AsRef<Path>>(path: P) -> Result<Box<dyn BufRead>> {
        let path = path.as_ref();

        if path.extension().and_then(|s| s.to_str()) == Some("gz") {
            let file = File::open(path)?;
            Ok(Box::new(BufReader::new(GzDecoder::new(file))))
        } else {
            let file = File::open(path)?;
            Ok(Box::new(BufReader::new(file)))
        }
    }
}

/// Zero-sized type that implements FastaReadable
/// This allows us to use the trait methods without needing an instance
pub struct FastaFile;

impl FastaReadable for FastaFile {}
