//! Utility functions for string processing and output handling
#![allow(dead_code)]

use anyhow::Result;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Helper function to read lines from a reader, handling non-UTF-8 gracefully
pub(crate) fn read_lines_lossy<R: BufRead>(reader: R) -> impl Iterator<Item = Result<String>> {
    reader.split(b'\n').map(|line_result| {
        line_result
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .map_err(|e| anyhow::anyhow!("IO error reading line: {}", e))
    })
}

/// Safe string slicing helper to avoid UTF-8 boundary panics
#[allow(dead_code)]
pub(crate) fn safe_slice_start(s: &str, byte_len: usize) -> Option<&str> {
    if byte_len > s.len() {
        return None;
    }
    // Check if the position is a valid UTF-8 boundary
    if s.is_char_boundary(byte_len) {
        Some(&s[..byte_len])
    } else {
        // Find the nearest char boundary before the target
        for i in (0..byte_len).rev() {
            if s.is_char_boundary(i) {
                return Some(&s[..i]);
            }
        }
        None
    }
}

/// Safe string splitting at byte position
#[allow(dead_code)]
pub(crate) fn safe_split_at(s: &str, byte_pos: usize) -> Option<(&str, &str)> {
    if byte_pos > s.len() {
        return None;
    }
    // Check if the position is a valid UTF-8 boundary
    if s.is_char_boundary(byte_pos) {
        Some(s.split_at(byte_pos))
    } else {
        // Find the nearest char boundary
        for i in (0..byte_pos).rev() {
            if s.is_char_boundary(i) {
                return Some(s.split_at(i));
            }
        }
        None
    }
}

/// Safely truncate a string to max_chars characters
pub(crate) fn safe_truncate(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        return s;
    }
    // Use char_indices to find proper boundary
    let mut end_byte = s.len();
    for (i, (byte_idx, _)) in s.char_indices().enumerate() {
        if i >= max_chars {
            end_byte = byte_idx;
            break;
        }
    }
    &s[..end_byte]
}

/// Helper function to stream output with proper carriage return handling
/// This captures LAMBDA's progress updates that use \r for same-line updates
#[allow(clippy::excessive_nesting)]
pub(crate) fn stream_output_with_progress<R: Read + Send + 'static>(
    mut reader: R,
    prefix: &'static str,
    progress_counter: Arc<AtomicUsize>,
    progress_bar: Option<indicatif::ProgressBar>,
    output_file: Option<PathBuf>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let lambda_verbose = std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok();
        let mut current_line = Vec::new(); // Changed from String to Vec<u8>
        let mut byte = [0u8; 1];
        let mut errors = Vec::new();

        // Open output file if specified
        let mut file_handle = output_file
            .as_ref()
            .and_then(|path| std::fs::File::create(path).ok());

        loop {
            match reader.read(&mut byte) {
                Ok(0) => {
                    // End of stream
                    if !current_line.is_empty() {
                        let line_str = String::from_utf8_lossy(&current_line); // Handle non-UTF-8
                        if lambda_verbose {
                            println!("  {}: {}", prefix, line_str);
                        } else if prefix.contains("stderr") && !line_str.trim().is_empty() {
                            errors.push(line_str.to_string());
                        }
                        // Write to file if specified
                        if let Some(ref mut file) = file_handle {
                            use std::io::Write;
                            let _ = writeln!(file, "{}", line_str);
                        }
                        std::io::stdout().flush().ok();
                    }
                    break;
                }
                Ok(_) => {
                    let ch = byte[0];

                    if ch == b'\r' {
                        // Carriage return - print current line and reset cursor
                        if !current_line.is_empty() {
                            let line_str = String::from_utf8_lossy(&current_line); // Handle non-UTF-8
                            if lambda_verbose {
                                print!("\r  {}: {}", prefix, line_str);
                                std::io::stdout().flush().ok();
                            }
                            // Track progress for structured output
                            // Try multiple patterns that LAMBDA might use
                            let debug_lambda = std::env::var("TALARIA_DEBUG_LAMBDA").is_ok();

                            if debug_lambda
                                && (line_str.contains("Query")
                                    || line_str.contains("%")
                                    || line_str.contains("Progress"))
                            {
                                // Try to extract percentage from various formats
                                if let Some(pct_pos) = line_str.rfind('%') {
                                    // Look for a number before the %
                                    if pct_pos > 0 {
                                        let before_pct = &line_str[..pct_pos];
                                        // Find the last number
                                        let num_start = before_pct
                                            .rfind(|c: char| !c.is_ascii_digit() && c != '.')
                                            .map(|i| i + 1)
                                            .unwrap_or(0);
                                        if let Ok(pct) = before_pct[num_start..].parse::<f32>() {
                                            let aligned = (pct * 100.0) as usize;
                                            progress_counter.store(aligned, Ordering::Relaxed);
                                            if let Some(ref pb) = progress_bar {
                                                pb.set_position(aligned as u64);
                                            }
                                        }
                                    }
                                }
                                // Also handle "X of Y" format
                                else if line_str.contains(" of ") {
                                    let parts: Vec<&str> = line_str.split_whitespace().collect();
                                    for (i, part) in parts.iter().enumerate() {
                                        if *part == "of" && i > 0 && i + 1 < parts.len() {
                                            if let (Ok(current), Ok(total)) = (
                                                parts[i - 1]
                                                    .trim_matches(|c: char| !c.is_ascii_digit())
                                                    .parse::<usize>(),
                                                parts[i + 1]
                                                    .trim_matches(|c: char| !c.is_ascii_digit())
                                                    .parse::<usize>(),
                                            ) {
                                                if total > 0 {
                                                    let pct = (current * 100) / total;
                                                    progress_counter.store(pct, Ordering::Relaxed);
                                                    if let Some(ref pb) = progress_bar {
                                                        pb.set_position(pct as u64);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Write to file if specified
                            if let Some(ref mut file) = file_handle {
                                use std::io::Write;
                                let _ = writeln!(file, "{}", line_str);
                            }
                            current_line.clear();
                        }
                    } else if ch == b'\n' {
                        // Newline - print and clear line
                        let line_str = String::from_utf8_lossy(&current_line); // Handle non-UTF-8
                        if lambda_verbose {
                            println!("  {}: {}", prefix, line_str);
                        } else if prefix.contains("stderr") && !line_str.trim().is_empty() {
                            // Save errors for reporting
                            errors.push(line_str.to_string());
                        }
                        // Write to file if specified
                        if let Some(ref mut file) = file_handle {
                            use std::io::Write;
                            let _ = writeln!(file, "{}", line_str);
                        }
                        current_line.clear();
                    } else {
                        // Regular character - accumulate
                        current_line.push(ch);
                    }
                }
                Err(_) => break,
            }
        }

        // Report critical errors at the end
        if !errors.is_empty() && !lambda_verbose {
            for error in errors.iter().take(5) {
                if error.contains("Error") || error.contains("error") {
                    eprintln!("  LAMBDA error: {}", error);
                }
            }
        }
    })
}
