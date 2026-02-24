# Compression

Read and write compressed archive files. Compression is detected automatically from the file extension—no special flags needed.

## Supported Formats

| Extension | Format | Status |
|-----------|--------|--------|
| `.gz` | gzip | ✅ Working |
| `.deflate` | deflate | ✅ Working |
| `.zlib` | zlib | ✅ Working |
| `.br` | brotli | ✅ Working |
| `.zst` | zstd | ⚠️ Not working yet |

**Note:** zstd support is planned but not yet implemented. Use one of the other formats for now.

## Example: gzip Workflow

Using the demo files in `docs/demo`:

### Create a compressed archive

```bash
json-archive -o v1.json.archive.gz v1.json
```

### Append to a compressed archive

```bash
json-archive v1.json.archive.gz v2.json
```

The tool reads the compressed archive, computes deltas, and writes it back compressed.

### Build complete history

```bash
json-archive -o v1.json.archive.gz v1.json v2.json v3.json
```

### Inspect the archive

All commands work transparently with compressed archives:

```bash
# View archive metadata
json-archive info v1.json.archive.gz

# Get state at specific observation
json-archive state --index 0 v1.json.archive.gz

# Get latest state
json-archive state v1.json.archive.gz
```

### Compare file sizes

```bash
# Create both versions
json-archive -o v1.json.archive v1.json v2.json v3.json
json-archive -o v1.json.archive.gz v1.json v2.json v3.json

# Compare sizes
ls -la v1.json.archive*
```

Typical compression ratios are 60-80% for JSON data.

## Caveats

### Temporary filesystem requirements

Compressed archives may require rewriting the entire file during updates. If `/tmp` is full or too small, updates can fail. Specify an output destination with `-o` to write elsewhere:

```bash
json-archive -o /path/with/space/data.json.archive.gz existing.archive.gz new-data.json
```

### When to use compression

Use compression when archives are large (hundreds of MB), storage is tight, or you're transferring files over networks.

Stick with uncompressed if you want to grep through archives directly, edit them by hand, or debug their contents.

## Building Without Compression

Compression libraries can be a security vulnerability vector. To build without compression support:

```bash
cargo install json-archive --no-default-features
```

The minimal build detects compressed files and gives a clear error explaining you need the full version or manual decompression.

## See Also

- [`json-archive info`](info-command.md) - View archive metadata
- [`json-archive state`](state-command.md) - Retrieve JSON at specific observations
- [File Format Specification](file-format-spec.md) - Archive format details
