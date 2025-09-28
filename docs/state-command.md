# State Command

The `json-archive state` command retrieves the JSON state at a specific observation

## Flags and Semantics:
Primary access methods:
- --id <observation-id>: Get state at specific observation (unambiguous, primary method)
- --index <n>: Get state at Nth observation in file order (convenience for using outout from info command, with caveat)

Timestamp-based access:
- --as-of <timestamp>: Most recent observation with timestamp ≤ given time ("state as of
 this moment")
- ---before <timestamp>: Most recent observation with timestamp < given time
(strictly before)
- --after <timestamp>: Earliest observation with timestamp > given time (first state
after)
- --latest: Most recent observation by timestamp (default if no flags given)

## Access Methods

You must specify exactly one access method to identify which observation's state to retrieve:

### By Observation ID (Recommended)
```bash
json-archive state --id <OBSERVATION_ID> file.archive
```
Gets the state at the observation with the specified ID. This is the most unambiguous method since observation IDs are unique within the archive.

Example:
```bash
json-archive state --id obs-c4636428-1400-44d7-b30f-1c080c608e3c data.json.archive
```

### By File Index
```bash
json-archive state --index <INDEX> file.archive
```
Gets the state at the Nth observation in file order (0-indexed). **Note:** Observations are not guaranteed to be in chronological order in the file.

Example:
```bash
json-archive state --index 0 data.json.archive    # Initial state (first observation)
json-archive state --index 1 data.json.archive    # Second observation
```

### By Timestamp

#### As-Of Timestamp
```bash
json-archive state --as-of <TIMESTAMP> file.archive
```
Gets the state from the most recent observation with timestamp ≤ the given time.

Example:
```bash
json-archive state --as-of "2025-01-15T10:05:00Z" data.json.archive
```

#### Right Before Timestamp
```bash
json-archive state --before <TIMESTAMP> file.archive
```
Gets the state from the most recent observation with timestamp < the given time (strictly before).

Example:
```bash
json-archive state --before "2025-01-15T10:05:00Z" data.json.archive
```

#### After Timestamp
```bash
json-archive state --after <TIMESTAMP> file.archive
```
Gets the state from the earliest observation with timestamp > the given time.

Example:
```bash
json-archive state --after "2025-01-15T10:05:00Z" data.json.archive
```

### Latest State (Default)
```bash
json-archive state --latest file.archive
# Or simply:
json-archive state file.archive
```
Gets the state from the observation with the latest timestamp. This is the default behavior when no other access method is specified.

## Timestamp Format

All timestamps must be in ISO-8601 format with UTC timezone:
- `2025-01-15T10:05:00Z`
- `2025-01-15T10:05:00.123Z` (with milliseconds)

## Output

The command outputs the JSON state as pretty-printed JSON to stdout:

```json
{
  "count": 10,
  "id": 1,
  "lastSeen": "2025-01-16",
  "name": "Bob",
  "score": 95,
  "tags": [
    "initial",
    "final"
  ]
}
```

## Error Cases

### Non-existent Observation ID
```bash
$ json-archive state --id nonexistent-id file.archive
error E030: Non-existent observation ID

I couldn't find an observation with ID 'nonexistent-id'

Use 'json-archive info' to see available observation IDs
```

### Out-of-bounds Index
```bash
$ json-archive state --index 10 file.archive
error E053: Array index out of bounds

Index 10 is out of bounds. The archive has 3 observations (0-2)

Use 'json-archive info' to see available observation indices
```

### Invalid Timestamp
```bash
$ json-archive state --as-of "invalid-timestamp" file.archive
error W012: Invalid timestamp

I couldn't parse the timestamp 'invalid-timestamp'. Please use ISO-8601 format like '2025-01-15T10:05:00Z'
```

### No Observations Match Timestamp
```bash
$ json-archive state --as-of "2020-01-01T00:00:00Z" file.archive
error E051: Path not found

No observations found as of 2020-01-01 00:00:00 UTC

Try using --after to find the first observation after this time
```

### Multiple Access Methods
```bash
$ json-archive state --id obs-123 --index 1 file.archive
error E022: Wrong field count

Please specify only one access method (--id, --index, --as-of, --right-before, --after, or --latest)

Examples:
json-archive state --id obs-123 file.archive
json-archive state --index 2 file.archive
json-archive state --as-of "2025-01-15T10:05:00Z" file.archive
```

## Implementation Details and Design Rationale

### Why Timestamp Access Uses More Memory

The timestamp-based flags (`--as-of`, `--before`, `--after`, `--latest`) are **memory-intensive** because they must read the entire archive file into memory. Here's why:

**The Problem**: Since observations are not guaranteed to be in chronological order in the file, we cannot use efficient seeking or leverage snapshots for optimization. To find "the most recent observation ≤ timestamp", we must:

1. Read every single observation in the file
2. Parse all their timestamps  
3. Sort them chronologically in memory
4. Find the target observation
5. Replay all events to reconstruct the state

This is fundamentally different from index-based access (`--index`, `--id`) which can potentially use snapshots and delta compression for efficiency.

### Why Files Aren't Chronologically Sorted

The tool accepts observations in any order because data collection is ad hoc. You don't always have perfect control over when or how observations are collected.

**Real scenario**: You have JSON files scattered across multiple hard drives from different systems. You can't mount all drives simultaneously, so you load them one at a time and absorb the data as you go. The observations end up out of chronological order, but that's fine - you consolidate first, then organize later.

**Another scenario**: You inherit data from multiple APIs and systems that were collecting observations over time. The timestamps are there, but the files weren't collected in chronological order. You want to merge everything into one archive without having to pre-sort.

**The approach**: "Octopus-style merging" - absorb all the data as-is, then make sense of it incrementally. The file format doesn't fight this workflow.

### Design Philosophy

Timestamp access works the way it does because the tool prioritizes data consolidation over performance optimization. When you use `--as-of` or `--latest`, you're asking the tool to figure out chronological relationships that may not exist in the file structure.

Index/ID access is direct - you're referencing observations by their position in the file or unique identifier. This is efficient because it doesn't require chronological analysis.

## See Also

- [`json-archive info`](info-command.md) - View archive metadata and observation timeline
- [File Format Specification](file-format-spec.md) - Details about the archive format
- [Getting Started Guide](../README.md) - Basic usage examples
