/// UniProt API client for fetching sequences by TaxID and proteome
use crate::bio::sequence::Sequence;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::Read;
use std::time::Duration;

/// UniProt API client
pub struct UniProtClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl UniProtClient {
    /// Create a new UniProt client
    pub fn new(base_url: &str) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .user_agent("Talaria/1.0")
            .build()?;

        Ok(Self {
            base_url: base_url.to_string(),
            client,
        })
    }

    /// Fetch sequences for a specific TaxID
    pub fn fetch_by_taxid(&self, taxid: u32) -> Result<Vec<Sequence>> {

        // Build query URL
        let query_url = format!(
            "{}/uniprotkb/stream?query=organism_id:{}&format=fasta&size=500",
            self.base_url, taxid
        );

        // Create progress bar
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("[{elapsed_precise}] {spinner:.green} {msg}")
                .unwrap()
        );
        pb.set_message(format!("  Downloading sequences for TaxID {}", taxid));
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        // Make request
        let response = self.client
            .get(&query_url)
            .send()
            .with_context(|| format!("Failed to fetch sequences for TaxID {}", taxid))?;

        if !response.status().is_success() {
            anyhow::bail!("UniProt API returned status: {}", response.status());
        }

        // Read response
        let mut body = String::new();
        response.take(100 * 1024 * 1024) // Limit to 100MB
            .read_to_string(&mut body)?;

        pb.finish_and_clear();

        // Parse FASTA and set taxon_id for all sequences
        let mut sequences = self.parse_fasta(&body)?;
        for seq in &mut sequences {
            seq.taxon_id = Some(taxid);
        }
        Ok(sequences)
    }

    /// Fetch sequences for multiple TaxIDs with optional progress callback
    pub fn fetch_by_taxids_with_progress<F>(
        &self,
        taxids: &[u32],
        mut progress_callback: F,
    ) -> Result<Vec<Sequence>>
    where
        F: FnMut(usize, u32, Option<usize>),
    {
        let mut all_sequences = Vec::new();

        for (i, &taxid) in taxids.iter().enumerate() {
            progress_callback(i + 1, taxid, None);

            match self.fetch_by_taxid(taxid) {
                Ok(sequences) => {
                    let count = sequences.len();
                    progress_callback(i + 1, taxid, Some(count));
                    all_sequences.extend(sequences);
                }
                Err(_e) => {
                    // Silently continue with other taxids
                    progress_callback(i + 1, taxid, Some(0));
                }
            }

            // Rate limiting - be nice to UniProt API
            if i < taxids.len() - 1 {
                std::thread::sleep(Duration::from_millis(500));
            }
        }

        Ok(all_sequences)
    }

    /// Fetch reference proteomes for an organism
    pub fn fetch_reference_proteome(&self, organism: &str) -> Result<Vec<Sequence>> {
        println!("▼ Fetching reference proteome for {}...", organism);

        // Build query URL for reference proteomes
        let query_url = format!(
            "{}/uniprotkb/stream?query=organism_name:\"{}\" AND proteome:reference&format=fasta&size=500",
            self.base_url, organism
        );

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("[{elapsed_precise}] {spinner:.green} {msg}")
                .unwrap()
        );
        pb.set_message(format!("Downloading reference proteome for {}", organism));
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let response = self.client
            .get(&query_url)
            .send()
            .with_context(|| format!("Failed to fetch reference proteome for {}", organism))?;

        if !response.status().is_success() {
            anyhow::bail!("UniProt API returned status: {}", response.status());
        }

        let mut body = String::new();
        response.take(100 * 1024 * 1024)
            .read_to_string(&mut body)?;

        pb.finish_with_message(format!("Downloaded reference proteome for {}", organism));

        self.parse_fasta(&body)
    }

    /// Fetch a specific proteome by ID
    pub fn fetch_proteome(&self, proteome_id: &str) -> Result<Vec<Sequence>> {
        println!("▼ Fetching proteome {}...", proteome_id);

        // Build query URL for specific proteome
        let query_url = format!(
            "{}/uniprotkb/stream?query=proteome:{}&format=fasta&size=500",
            self.base_url, proteome_id
        );

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("[{elapsed_precise}] {spinner:.green} {msg}")
                .unwrap()
        );
        pb.set_message(format!("Downloading proteome {}", proteome_id));
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let response = self.client
            .get(&query_url)
            .send()
            .with_context(|| format!("Failed to fetch proteome {}", proteome_id))?;

        if !response.status().is_success() {
            anyhow::bail!("UniProt API returned status: {}", response.status());
        }

        let mut body = String::new();
        response.take(100 * 1024 * 1024)
            .read_to_string(&mut body)?;

        pb.finish_with_message(format!("Downloaded proteome {}", proteome_id));

        self.parse_fasta(&body)
    }

    /// Parse FASTA format into sequences
    fn parse_fasta(&self, content: &str) -> Result<Vec<Sequence>> {
        let mut sequences = Vec::new();
        let mut current_id = String::new();
        let mut current_desc = None;
        let mut current_data = Vec::new();

        for line in content.lines() {
            if line.starts_with('>') {
                // Save previous sequence if exists
                if !current_id.is_empty() && !current_data.is_empty() {
                    sequences.push(Sequence {
                        id: current_id.clone(),
                        description: current_desc.clone(),
                        sequence: current_data.clone(),
                        taxon_id: None,
                    });
                }

                // Parse new header
                let header = &line[1..];
                let parts: Vec<&str> = header.splitn(2, ' ').collect();
                current_id = parts[0].to_string();
                current_desc = parts.get(1).map(|s| s.to_string());
                current_data.clear();
            } else if !line.trim().is_empty() {
                current_data.extend(line.trim().bytes());
            }
        }

        // Save last sequence
        if !current_id.is_empty() && !current_data.is_empty() {
            sequences.push(Sequence {
                id: current_id,
                description: current_desc,
                sequence: current_data,
                taxon_id: None,
            });
        }

        Ok(sequences)
    }
}

/// Parse TaxIDs from various input formats
pub fn parse_taxids(input: &str) -> Result<Vec<u32>> {
    let mut taxids = Vec::new();

    // Handle comma-separated values
    for part in input.split(',') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            let taxid = trimmed.parse::<u32>()
                .with_context(|| format!("Invalid TaxID: {}", trimmed))?;
            taxids.push(taxid);
        }
    }

    Ok(taxids)
}

/// Read TaxIDs from a file (one per line)
pub fn read_taxids_from_file(path: &std::path::Path) -> Result<Vec<u32>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open(path)
        .with_context(|| format!("Failed to open TaxID list file: {}", path.display()))?;

    let mut taxids = Vec::new();
    let reader = BufReader::new(file);

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let taxid = trimmed.parse::<u32>()
            .with_context(|| format!("Invalid TaxID on line {}: {}", line_num + 1, trimmed))?;
        taxids.push(taxid);
    }

    Ok(taxids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_taxids() {
        // Test comma-separated
        let taxids = parse_taxids("9606, 10090, 7227").unwrap();
        assert_eq!(taxids, vec![9606, 10090, 7227]);

        // Test single value
        let taxids = parse_taxids("9606").unwrap();
        assert_eq!(taxids, vec![9606]);

        // Test with extra spaces
        let taxids = parse_taxids("  9606  ,  10090  ").unwrap();
        assert_eq!(taxids, vec![9606, 10090]);
    }

    #[test]
    fn test_parse_fasta() {
        let client = UniProtClient::new("https://rest.uniprot.org").unwrap();

        let fasta = ">sp|P12345|PROT_HUMAN Protein description\n\
                     MKWVTFISLLFLFSSAYSRGVFRR\n\
                     DTHKSEIAHRFKDLGEEHFKGLVL\n\
                     >sp|Q67890|PROT_MOUSE Another protein\n\
                     MKWVTFISLLFLFSSAYS";

        let sequences = client.parse_fasta(fasta).unwrap();
        assert_eq!(sequences.len(), 2);
        assert_eq!(sequences[0].id, "sp|P12345|PROT_HUMAN");
        assert_eq!(sequences[0].description.as_ref().unwrap(), "Protein description");
        assert_eq!(sequences[1].id, "sp|Q67890|PROT_MOUSE");
    }
}