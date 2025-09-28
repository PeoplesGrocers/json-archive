#![no_main]

use libfuzzer_sys::fuzz_target;
use json_archive::{ArchiveReader, ReadMode};
use std::io::Write;
use tempfile::NamedTempFile;

fuzz_target!(|data: &[u8]| {
    // Write the random bytes to a temporary file
    if let Ok(mut temp_file) = NamedTempFile::new() {
        if temp_file.write_all(data).is_ok() {
            // Try to read the file with both validation modes
            for mode in [ReadMode::FullValidation, ReadMode::AppendSeek] {
                if let Ok(reader) = ArchiveReader::new(temp_file.path(), mode) {
                    // The read operation should never panic, regardless of input
                    // It should either succeed or return an error gracefully
                    let _ = reader.read(temp_file.path());
                }
            }
        }
    }
});