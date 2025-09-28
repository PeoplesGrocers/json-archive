#![no_main]

use libfuzzer_sys::fuzz_target;
use json_archive::{ArchiveReader, ReadMode};
use std::io::Write;
use tempfile::NamedTempFile;

fn create_archive_content(data: &[u8]) -> Vec<u8> {
    // Start with a simple base archive
    let base = r#"{"version":1,"created":"2025-01-01T00:00:00Z","initial":{"id":1}}
["observe", "obs-1", "2025-01-01T00:00:00Z", 1]
["add", "/name", "test", "obs-1"]"#;
    
    let mut result = base.as_bytes().to_vec();
    
    // Apply simple mutations based on fuzz input
    if data.is_empty() {
        return result;
    }
    
    // Keep mutations small and realistic
    let max_size = 4096; // 4KB limit
    
    for (i, &byte) in data.iter().take(16).enumerate() {
        if result.len() > max_size {
            break;
        }
        
        match byte % 8 {
            0 => {
                // Truncate at random position
                let pos = (byte as usize) % result.len().max(1);
                result.truncate(pos);
            }
            1 => {
                // Insert invalid UTF-8
                let pos = (byte as usize) % (result.len() + 1);
                result.insert(pos, 0xFF);
            }
            2 => {
                // Corrupt a quote
                if let Some(pos) = result.iter().position(|&b| b == b'"') {
                    result[pos] = b'X';
                }
            }
            3 => {
                // Insert extra newline
                let pos = (byte as usize) % (result.len() + 1);
                result.insert(pos, b'\n');
            }
            4 => {
                // Corrupt JSON bracket
                if let Some(pos) = result.iter().position(|&b| b == b'[' || b == b'{') {
                    result[pos] = b'?';
                }
            }
            5 => {
                // Insert random byte
                let pos = (byte as usize) % (result.len() + 1);
                result.insert(pos, byte);
            }
            6 => {
                // Remove a character
                if !result.is_empty() {
                    let pos = (byte as usize) % result.len();
                    result.remove(pos);
                }
            }
            _ => {
                // Add some garbage line
                let insertion = format!("\n[\"garbage\", {}]", i);
                let pos = (byte as usize) % (result.len() + 1);
                result.splice(pos..pos, insertion.bytes());
            }
        }
    }
    
    result
}

fuzz_target!(|data: &[u8]| {
    let archive_content = create_archive_content(data);
    
    if let Ok(mut temp_file) = NamedTempFile::new() {
        if temp_file.write_all(&archive_content).is_ok() {
            // Test both validation modes
            for mode in [ReadMode::FullValidation, ReadMode::AppendSeek] {
                if let Ok(reader) = ArchiveReader::new(temp_file.path(), mode) {
                    let result = reader.read(temp_file.path());
                    
                    // Should never panic, regardless of input malformation
                    match result {
                        Ok(read_result) => {
                            // Basic invariants that should hold for any successful parse
                            let _ = &read_result.final_state;
                            let _ = &read_result.diagnostics;
                            
                            // Observation count should be reasonable
                            assert!(read_result.observation_count < 100000);
                            
                            // If we have diagnostics, they should be well-formed
                            for diagnostic in read_result.diagnostics.diagnostics() {
                                assert!(!diagnostic.description.is_empty());
                            }
                        },
                        Err(_) => {
                            // It's fine for the parser to reject malformed input
                            // Just make sure it doesn't panic
                        }
                    }
                }
            }
        }
    }
});