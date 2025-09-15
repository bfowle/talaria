use crate::bio::sequence::Sequence;
use crate::TalariaError;
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
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

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
fn parse_sequence(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    let mut sequence = Vec::new();
    let mut remaining = input;
    
    while !remaining.is_empty() && remaining[0] != b'>' {
        // Parse a line of sequence data
        let (rest, line) = take_till::<_, _, nom::error::Error<_>>(|c: u8| c == b'\n' || c == b'\r')(remaining)?;
        let (rest, _) = opt(line_ending)(rest)?;
        
        // Add non-whitespace characters to sequence
        for &c in line {
            if !c.is_ascii_whitespace() {
                sequence.push(c.to_ascii_uppercase());
            }
        }
        
        remaining = rest;
    }
    
    Ok((remaining, sequence))
}

/// Parse a single FASTA record
fn parse_record(input: &[u8]) -> IResult<&[u8], Sequence> {
    let (input, (id, description)) = parse_header(input)?;
    let (input, sequence) = parse_sequence(input)?;
    
    let mut seq = Sequence::new(id.to_string(), sequence);
    if let Some(desc) = description {
        seq = seq.with_description(desc.to_string());
    }
    
    // Extract taxon ID if present in description
    if let Some(desc) = &seq.description {
        if let Some(taxon) = extract_taxon_id(desc) {
            seq = seq.with_taxon(taxon);
        }
    }
    
    Ok((input, seq))
}

/// Extract taxon ID from description (handles various formats)
fn extract_taxon_id(description: &str) -> Option<u32> {
    // Try common patterns: OX=12345, TaxID=12345, taxon:12345
    let patterns = [
        ("OX=", ""),
        ("TaxID=", ""),
        ("taxon:", ""),
        ("tax_id=", ""),
    ];
    
    for (prefix, _) in patterns {
        if let Some(pos) = description.find(prefix) {
            let start = pos + prefix.len();
            let taxon_str: String = description[start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            
            if let Ok(taxon) = taxon_str.parse::<u32>() {
                return Some(taxon);
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

        // Parse sequence
        let (rest, seq_data) = parse_sequence(rest)
            .map_err(|_| TalariaError::Parse("Failed to parse FASTA sequence".to_string()))?;

        let mut seq = Sequence::new(id.to_string(), seq_data);
        if let Some(desc) = description {
            seq.description = Some(desc.to_string());
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
                return Err(TalariaError::Parse(format!("Failed to parse FASTA: {:?}", e)));
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

/// Write sequences to any writer
fn write_fasta_to_writer<W: Write>(writer: &mut W, sequences: &[Sequence]) -> Result<(), TalariaError> {
    for seq in sequences {
        writeln!(writer, "{}", seq.header())?;
        
        // Write sequence in 80-character lines
        for chunk in seq.sequence.chunks(80) {
            writeln!(writer, "{}", String::from_utf8_lossy(chunk))?;
        }
    }
    Ok(())
}

/// Parse FASTA in parallel chunks for large files (supports .gz compression)
pub fn parse_fasta_parallel<P: AsRef<Path>>(path: P, chunk_size: usize) -> Result<Vec<Sequence>, TalariaError> {
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
        if pos > 0 && mmap[pos] == b'>' && (pos == 0 || mmap[pos - 1] == b'\n') {
            if boundaries.last().map_or(true, |&last| pos - last >= chunk_size) {
                boundaries.push(pos);
            }
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
                        return Err(TalariaError::Parse(format!("Failed to parse FASTA chunk: {:?}", e)));
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
}