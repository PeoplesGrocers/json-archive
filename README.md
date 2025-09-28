# json-archive

A practical CLI tool for tracking JSON file changes over time. Instead of keeping multiple copies of JSON files, this creates compact delta-based archives that preserve the complete history.

## What it does

This tool solves a simple problem: you have a JSON file that changes regularly, and you want to track its history without storing dozens of full copies. 

`json-archive` creates a `.json.archive` file next to your original JSON file. Each time you run the tool, it calculates only what changed and appends those deltas to the archive. You get complete history with minimal storage overhead.

The archive format is human-readable JSONL (not binary), making it easy to inspect, debug, and pipe into other scripts or web visualizations.

## Quick example

```bash
# Create initial archive from data.json (infers output: data.json.archive)
json-archive data.json

# Later, append changes to existing archive
json-archive data.json.archive data.json

# Or let it infer again (won't overwrite without --force)
json-archive data.json  # Safe: won't overwrite existing data.json.archive
```

## Real-world use case

Perfect for tracking YouTube video metadata over time:

```bash
# Download video info with yt-dlp
yt-dlp --write-info-json -o "%(id)s.%(ext)s" "https://youtube.com/watch?v=..."

# Create initial archive (creates videoID.info.json.archive)
json-archive videoID.info.json

# Later, append new observations to existing archive
json-archive videoID.info.json.archive videoID.info.json

# Or safely re-run (won't overwrite existing archive)
json-archive videoID.info.json

# Run daily in a cron job to capture changes
# The archive preserves your title/description experiments and view count history
```

## Design philosophy

**Hackable over efficient**: The file format prioritizes human readability and scriptability over binary compactness. You can:

- Open archives in any text editor
- Grep through them for specific changes  
- Parse them in JavaScript without special libraries
- Pipe them through standard Unix tools

**Minimal workflow changes**: Archive files sit next to your original JSON files with a `.archive` extension. Your existing scripts need minimal modification.

### Compression support (as a concession)

While the core design keeps things simple and readable, the tool does work with compressed archives as a practical concession for those who need it. You can read from and write to gzip, brotli, and zlib compressed files without special flags.

**Important caveat**: Compressed archives may require rewriting the entire file during updates (depending on the compression format). If your temporary filesystem is full or too small, updates can fail. In that case, manually specify an output destination with `-o` to write the new archive elsewhere.

This works fine for the happy path with archive files up to a few hundred megabytes, but contradicts the "keep it simple" design philosophy - it's included because it's practically useful.

## Archive format

The format is JSONL with delta-based changes using [JSON Pointer](https://tools.ietf.org/html/rfc6901) paths. For complete technical details about the file format, see the [file format specification](docs/file-format-spec.md).

```jsonl
{"version": 1, "created": "2025-01-15T10:00:00Z", "initial": {"views": 100, "title": "My Video"}}
# First observation  
["observe", "obs-001", "2025-01-15T10:05:00Z", 2]
["change", "/views", 100, 150, "obs-001"]
["change", "/title", "My Video", "My Awesome Video", "obs-001"]
# Second observation
["observe", "obs-002", "2025-01-15T11:00:00Z", 1]  
["change", "/views", 150, 200, "obs-002"]
```

Each observation records:
- What changed (using JSON Pointer paths like `/views`)
- The old and new values
- When it happened
- A unique observation ID

## Commands

The tool infers behavior from filenames:

### Documentation

- [Info command](docs/info-command.md) - View archive metadata and observation timeline
- [State command](docs/state-command.md) - Retrieve JSON state at specific observations
- [File format specification](docs/file-format-spec.md) - Technical details about the archive format

### Creating archives

```bash
# Create archive from JSON files (output inferred from first filename)
json-archive file1.json file2.json file3.json
# Creates: file1.json.archive

# Won't overwrite existing archives (safe to re-run)
json-archive data.json  # Won't overwrite data.json.archive if it exists

# Force overwrite existing archive
json-archive --force data.json

# Specify custom output location
json-archive -o custom.archive data.json
```

### Appending to archives

```bash
# First file is archive, rest are appended
json-archive existing.json.archive new1.json new2.json

# Works with any mix of files
json-archive data.json.archive updated-data.json
```

### Additional options

```bash
# Add snapshots after 10 observations instead of default of 100 for faster append operations
json-archive -s 50 data.json

# Add source metadata
json-archive --source "youtube-metadata" data.json
```

## Installation

```bash
cargo install json-archive
```

Or build from source:

```bash
git clone <repo>
cd json-archive
cargo build --release
```

## File naming convention

Archives use the `.json.archive` extension by default:

- `data.json` -> `data.json.archive`
- `video.info.json` -> `video.info.json.archive`
- `config.json` -> `config.json.archive`

This makes it immediately clear which files are archives and which are source files.


## Error handling

The tool uses descriptive diagnostics instead of cryptic error codes:

```
error: I couldn't find the input file: missing.json
  |
  = help: Make sure the file path is correct and the file exists.
          Check for typos in the filename.
```

Diagnostics are categorized as Fatal, Warning, or Info, and the tool exits with non-zero status only for fatal errors.

## Performance characteristics

- **Memory usage**: Bounded by largest single JSON file, not archive size
- **Append speed**: Fast - only computes deltas, doesn't re-read entire archive
- **Read speed**: Linear scan, but snapshots allow seeking to recent state
- **File size**: Typically 10-30% the size of storing all JSON copies

For very large archives, consider using snapshots (`-s` flag) to enable faster seeking.

## Browser compatibility

Archives can be loaded directly in web applications:

```javascript
// Parse archive in browser
fetch('data.json.archive')
  .then(response => response.text())
  .then(text => {
    const lines = text.split('\n');
    const header = JSON.parse(lines[0]);
    const events = lines.slice(1)
      .filter(line => line && !line.startsWith('#'))
      .map(line => JSON.parse(line));
    
    // Replay history, build visualizations, etc.
  });
```

The format uses only standard JSON. No special parsing required.

## Contributing

This is a practical tool built for real workflow needs. Contributions welcome, especially:

- Additional CLI commands (validate, info, extract)
- Performance optimizations for large archives
- More compression format support
- Better diff algorithms for arrays

## License

---

*Built with Rust for reliability and performance. Designed to be simple enough to understand, powerful enough to be useful.*
