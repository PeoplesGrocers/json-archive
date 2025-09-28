# Info Command

Shows what's in an archive file: metadata, observation timeline, and file statistics.

## Basic Usage

```bash
json-archive info file.archive
json-archive info --output json file.archive
```

The command reads the entire archive file to collect observation data, so expect memory usage proportional to file size.

## Output Modes

### Human-readable (default)
```bash
json-archive info docs/demo/v1.json.archive
```

```
Archive: docs/demo/v1.json.archive
Created: Sun 15:23:40 28-Sep-2025

3 observations from Sun 15:23:40 28-Sep-2025 to Sun 15:23:40 28-Sep-2025

  #  Observation ID                    Date & Time                  Changes  JSON Size
────────────────────────────────────────────────────────────────────────────────────────
   0  (initial)                         Sun 15:23:40 28-Sep-2025   -        52 bytes 
   1  obs-c4636428-1400-44...           Sun 15:23:40 28-Sep-2025   3        86 bytes 
   2  obs-389b4a7c-4d78-42...           Sun 15:23:40 28-Sep-2025   7        94 bytes 

Total archive size: 1.1 KB (0 snapshots)

To get the JSON value at a specific observation:
  json-archive state --index <#> docs/demo/v1.json.archive
  json-archive state --id <observation-id> docs/demo/v1.json.archive

Examples:
  json-archive state --index 0 docs/demo/v1.json.archive    # Get initial state
  json-archive state --index 2 docs/demo/v1.json.archive    # Get state after observation 2
```

Use this mode when you want to quickly understand what's in an archive or debug observation timelines.

### JSON output

Use JSON output for scripting, monitoring, or feeding data into other tools.

```bash
json-archive info --output json docs/demo/v1.json.archive
```

```json
{
  "archive": "docs/demo/v1.json.archive",
  "created": "2025-09-28T15:23:40.633960+00:00",
  "file_size": 1096,
  "snapshot_count": 0,
  "observations": [
    {
      "index": 0,
      "id": "initial",
      "timestamp": "2025-09-28T15:23:40.633960+00:00",
      "changes": 0,
      "json_size": 52
    },
    {
      "index": 1,
      "id": "obs-c4636428-1400-44d7-b30f-1c080c608e3c",
      "timestamp": "2025-09-28T15:23:40.634520+00:00",
      "changes": 3,
      "json_size": 86
    }
  ]
}
```

## Field Reference

### Human-readable fields
- **#**: Index number (0-indexed) for use with `--index` flag
- **Observation ID**: Unique identifier for use with `--id` flag  
- **Date & Time**: When the observation was recorded
- **Changes**: Number of fields modified (dash for initial state)
- **JSON Size**: Size in bytes of the reconstructed JSON at this observation

### JSON output fields
- **archive**: File path
- **created**: Archive creation timestamp (ISO-8601)
- **file_size**: Archive file size in bytes
- **snapshot_count**: Number of snapshots for seeking optimization
- **observations[]**: Array of observation metadata
  - **index**: 0-based position for `--index` access
  - **id**: Unique ID for `--id` access ("initial" for index 0)
  - **timestamp**: ISO-8601 timestamp
  - **changes**: Change count (0 for initial)
  - **json_size**: Reconstructed JSON size in bytes

## Practical Use Cases

### Monitoring archive growth
```bash
# Archive size and observation count
json-archive info --output json data.json.archive | jq '{file_size, observation_count: (.observations | length)}'
```

### Finding large observations
```bash
# Observations with JSON size > 1KB
json-archive info --output json data.json.archive | jq '.observations[] | select(.json_size > 1024)'
```

## Performance Characteristics

- **Memory usage**: Loads entire archive into memory to reconstruct states
- **I/O pattern**: Single full file read, no seeking
- **CPU usage**: Minimal - mostly JSON parsing and formatting

For archives larger than available RAM, consider using [`json-archive state`](state-command.md) with specific observation IDs instead of getting full timeline info.

## Error Cases

### Missing archive file
```bash
$ json-archive info nonexistent.archive
error E051: Path not found

I couldn't find the archive file: nonexistent.archive

Make sure the file path is correct and the file exists.
Check for typos in the filename.
```

### Corrupt archive header
```bash
$ json-archive info corrupted.archive  
error E003: Missing header

I couldn't parse the header: unexpected character at line 1

The archive file appears to be corrupted or not a valid json-archive file.
```

### Invalid output format
```bash
$ json-archive info --output xml data.json.archive
# Silently falls back to human-readable format
# (no validation on output parameter)
```

## Known Issues

**Command reference bug**: The human-readable output currently shows `json-archive get` commands in the usage examples, but the correct command is `json-archive state`. This is a display bug - the actual functionality is unaffected.

## See Also

- [`json-archive state`](state-command.md) - Retrieve JSON data at specific observations
- [File Format Specification](file-format-spec.md) - Archive format details
- [Getting Started Guide](../README.md) - Basic usage examples
