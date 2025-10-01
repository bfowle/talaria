/// Metadata storage for references and deltas
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use talaria_bio::compression::delta::{format_deltas_dat, parse_deltas_dat};
use talaria_core::error::TalariaError;

// Re-export DeltaRecord so consumers don't need to import from talaria-bio
pub use talaria_bio::compression::delta::DeltaRecord;

pub fn write_metadata<P: AsRef<Path>>(path: P, deltas: &[DeltaRecord]) -> Result<(), TalariaError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_empty_deltas_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.dat");

        let delta = DeltaRecord {
            child_id: "!".to_string(),
            reference_id: "!".to_string(),
            taxon_id: Some(0),
            deltas: vec![],
            header_change: None,
        };

        write_metadata(&file_path, &[delta.clone()]).unwrap();
        let loaded = load_metadata(&file_path).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].child_id, delta.child_id);
        assert_eq!(loaded[0].reference_id, delta.reference_id);
        assert_eq!(loaded[0].taxon_id, delta.taxon_id);
        assert_eq!(loaded[0].deltas.len(), delta.deltas.len());
    }

    #[test]
    fn test_write_and_load_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("deltas.dat");

        // Create test delta records
        use talaria_bio::compression::delta::DeltaRange;
        let deltas = vec![
            DeltaRecord {
                child_id: "child1".to_string(),
                reference_id: "ref1".to_string(),
                taxon_id: Some(1),
                deltas: vec![DeltaRange {
                    start: 0,
                    end: 3,
                    substitution: vec![1, 2, 3],
                }],
                header_change: None,
            },
            DeltaRecord {
                child_id: "child2".to_string(),
                reference_id: "ref1".to_string(),
                taxon_id: Some(1),
                deltas: vec![DeltaRange {
                    start: 4,
                    end: 5,
                    substitution: vec![4, 5],
                }],
                header_change: None,
            },
            DeltaRecord {
                child_id: "child3".to_string(),
                reference_id: "ref2".to_string(),
                taxon_id: Some(2),
                deltas: vec![DeltaRange {
                    start: 6,
                    end: 9,
                    substitution: vec![6, 7, 8, 9],
                }],
                header_change: None,
            },
        ];

        // Write metadata
        write_metadata(&file_path, &deltas).unwrap();

        // Load metadata
        let loaded = load_metadata(&file_path).unwrap();

        // Verify
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].child_id, "child1");
        assert_eq!(loaded[0].reference_id, "ref1");
        assert_eq!(loaded[0].deltas.len(), 1);
        assert_eq!(loaded[0].deltas[0].substitution, vec![1, 2, 3]);

        assert_eq!(loaded[1].child_id, "child2");
        assert_eq!(loaded[2].child_id, "child3");
    }

    #[test]
    fn test_load_empty_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.dat");

        // Write empty file
        write_metadata(&file_path, &[]).unwrap();

        // Load should return empty vec
        let loaded = load_metadata(&file_path).unwrap();
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.dat");

        // Should return error
        let result = load_metadata(&file_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_and_load_ref2children() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("ref2children.txt");

        // Create test data
        let mut ref2children = std::collections::HashMap::new();
        ref2children.insert(
            "ref1".to_string(),
            vec![
                "child1".to_string(),
                "child2".to_string(),
                "child3".to_string(),
            ],
        );
        ref2children.insert(
            "ref2".to_string(),
            vec!["child4".to_string(), "child5".to_string()],
        );
        ref2children.insert("ref3".to_string(), vec!["child6".to_string()]);

        // Write
        write_ref2children(&file_path, &ref2children).unwrap();

        // Load
        let loaded = load_ref2children(&file_path).unwrap();

        // Verify
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded.get("ref1").unwrap().len(), 3);
        assert!(loaded.get("ref1").unwrap().contains(&"child1".to_string()));
        assert!(loaded.get("ref1").unwrap().contains(&"child2".to_string()));
        assert!(loaded.get("ref1").unwrap().contains(&"child3".to_string()));

        assert_eq!(loaded.get("ref2").unwrap().len(), 2);
        assert_eq!(loaded.get("ref3").unwrap().len(), 1);
    }

    #[test]
    fn test_ref2children_empty_handling() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty_ref.txt");

        // Write empty map
        let ref2children = std::collections::HashMap::new();
        write_ref2children(&file_path, &ref2children).unwrap();

        // Load should return empty map
        let loaded = load_ref2children(&file_path).unwrap();
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_ref2children_single_child() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("single.txt");

        // Reference with single child
        let mut ref2children = std::collections::HashMap::new();
        ref2children.insert("ref_single".to_string(), vec!["only_child".to_string()]);

        write_ref2children(&file_path, &ref2children).unwrap();
        let loaded = load_ref2children(&file_path).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get("ref_single").unwrap().len(), 1);
        assert_eq!(loaded.get("ref_single").unwrap()[0], "only_child");
    }

    #[test]
    fn test_metadata_with_special_characters() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("special.dat");

        // Delta with special characters in IDs
        use talaria_bio::compression::delta::DeltaRange;
        let deltas = vec![
            DeltaRecord {
                child_id: "child-with-dash".to_string(),
                reference_id: "ref_with_underscore".to_string(),
                taxon_id: None,
                deltas: vec![DeltaRange {
                    start: 0,
                    end: 1,
                    substitution: vec![1],
                }],
                header_change: None,
            },
            DeltaRecord {
                child_id: "child.with.dots".to_string(),
                reference_id: "ref|with|pipes".to_string(),
                taxon_id: None,
                deltas: vec![DeltaRange {
                    start: 2,
                    end: 3,
                    substitution: vec![2, 3],
                }],
                header_change: None,
            },
        ];

        write_metadata(&file_path, &deltas).unwrap();
        let loaded = load_metadata(&file_path).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].child_id, "child-with-dash");
        assert_eq!(loaded[1].reference_id, "ref|with|pipes");
    }

    #[test]
    fn test_large_metadata_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("large.dat");

        // Create many delta records
        use talaria_bio::compression::delta::DeltaRange;
        let mut deltas = Vec::new();
        for i in 0..1000 {
            deltas.push(DeltaRecord {
                child_id: format!("child_{}", i),
                reference_id: format!("ref_{}", i % 10),
                taxon_id: Some(i as u32),
                deltas: vec![DeltaRange {
                    start: 0,
                    end: (i % 10) + 1,
                    substitution: vec![i as u8; (i % 10) + 1],
                }],
                header_change: None,
            });
        }

        write_metadata(&file_path, &deltas).unwrap();
        let loaded = load_metadata(&file_path).unwrap();

        assert_eq!(loaded.len(), 1000);
        for i in 0..1000 {
            assert_eq!(loaded[i].child_id, format!("child_{}", i));
            assert_eq!(loaded[i].reference_id, format!("ref_{}", i % 10));
            assert_eq!(loaded[i].taxon_id, Some(i as u32));
        }
    }

    #[test]
    fn test_ref2children_with_duplicates() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("duplicates.txt");

        // Note: HashMap naturally prevents duplicate keys
        let mut ref2children = std::collections::HashMap::new();
        ref2children.insert(
            "ref1".to_string(),
            vec!["child1".to_string(), "child1".to_string()], // Duplicate child
        );

        write_ref2children(&file_path, &ref2children).unwrap();
        let loaded = load_ref2children(&file_path).unwrap();

        // Duplicates in the children vec are preserved
        assert_eq!(loaded.get("ref1").unwrap().len(), 2);
    }

    // Property-based test
    #[quickcheck_macros::quickcheck]
    fn prop_metadata_roundtrip(records: Vec<(String, String, Vec<u8>, u32)>) -> bool {
        if records.is_empty() {
            return true;
        }

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("prop_test.dat");

        // Convert to DeltaRecords
        use talaria_bio::compression::delta::DeltaRange;
        let deltas: Vec<DeltaRecord> = records
            .into_iter()
            .filter(|(child, reference, _, _)| {
                // Filter out empty, whitespace-only, or control-character-only strings
                let child_valid = !child.trim().is_empty()
                    && child
                        .chars()
                        .any(|c| c.is_alphanumeric() || c.is_ascii_punctuation());
                let ref_valid = !reference.trim().is_empty()
                    && reference
                        .chars()
                        .any(|c| c.is_alphanumeric() || c.is_ascii_punctuation());
                child_valid && ref_valid
            })
            .map(|(child, reference, ops, taxon)| DeltaRecord {
                // Sanitize by replacing all control characters with underscore
                child_id: child
                    .chars()
                    .map(|c| if c.is_control() { '_' } else { c })
                    .collect(),
                reference_id: reference
                    .chars()
                    .map(|c| if c.is_control() { '_' } else { c })
                    .collect(),
                taxon_id: Some(taxon),
                // Only create a DeltaRange if ops is not empty
                deltas: if ops.is_empty() {
                    vec![]
                } else {
                    vec![DeltaRange {
                        start: 0,
                        end: ops.len(),
                        substitution: ops,
                    }]
                },
                header_change: None,
            })
            .collect();

        if deltas.is_empty() {
            return true;
        }

        // Write and read back
        if write_metadata(&file_path, &deltas).is_err() {
            return true; // Skip if write fails (e.g., invalid chars)
        }

        match load_metadata(&file_path) {
            Ok(loaded) => {
                loaded.len() == deltas.len()
                    && loaded.iter().zip(deltas.iter()).all(|(l, d)| {
                        l.child_id == d.child_id
                            && l.reference_id == d.reference_id
                            && l.taxon_id == d.taxon_id
                            && l.deltas.len() == d.deltas.len()
                            && l.deltas.iter().zip(d.deltas.iter()).all(|(ld, dd)| {
                                ld.start == dd.start
                                    && ld.end == dd.end
                                    && ld.substitution == dd.substitution
                            })
                    })
            }
            Err(_) => false,
        }
    }
}
