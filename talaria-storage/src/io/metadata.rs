/// Metadata storage for references and deltas
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use talaria_core::error::TalariaError;
use talaria_bio::compression::delta::{format_deltas_dat, parse_deltas_dat};

// Re-export DeltaRecord so consumers don't need to import from talaria-bio
pub use talaria_bio::compression::delta::DeltaRecord;

pub fn write_metadata<P: AsRef<Path>>(
    path: P,
    deltas: &[DeltaRecord],
) -> Result<(), TalariaError> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    for delta in deltas {
        writeln!(writer, "{}", format_deltas_dat(delta))?;
    }

    writer.flush()?;
    Ok(())
}

pub fn load_metadata<P: AsRef<Path>>(path: P) -> Result<Vec<DeltaRecord>, TalariaError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut deltas = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if !line.is_empty() {
            deltas.push(parse_deltas_dat(&line)?);
        }
    }

    Ok(deltas)
}

pub fn write_ref2children<P: AsRef<Path>>(
    path: P,
    ref2children: &std::collections::HashMap<String, Vec<String>>,
) -> Result<(), TalariaError> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    for (reference, children) in ref2children {
        write!(writer, "{}", reference)?;
        for child in children {
            write!(writer, "\t{}", child)?;
        }
        writeln!(writer)?;
    }

    writer.flush()?;
    Ok(())
}

pub fn load_ref2children<P: AsRef<Path>>(
    path: P,
) -> Result<std::collections::HashMap<String, Vec<String>>, TalariaError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut ref2children = std::collections::HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split('\t').collect();

        if !parts.is_empty() {
            let reference = parts[0].to_string();
            let children: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
            ref2children.insert(reference, children);
        }
    }

    Ok(ref2children)
}
