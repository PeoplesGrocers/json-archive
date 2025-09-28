# json-archive Demo

This directory contains example JSON files that demonstrate how to use the `json-archive` tool to track changes over time.

## Sample Data

The demo uses three snapshots of the same JSON object as it evolves:

### [v1.json](v1.json)
```json
{"id": 1, "name": "Alice", "count": 0, "tags": ["initial"]}
```

### [v2.json](v2.json) 
```json
{"id": 1, "name": "Alice", "count": 5, "tags": ["initial", "updated"], "lastSeen": "2025-01-15"}
```

### [v3.json](v3.json)
```json
{"id": 1, "name": "Bob", "count": 10, "tags": ["updated", "final"], "lastSeen": "2025-01-16", "score": 95}
```

## Basic Usage

### 1. Create Initial Archive

```bash
json-archive v1.json
```

**What happens:**
- Creates `v1.json.archive` with the initial state
- Leaves `v1.json` untouched
- The archive contains a header with the complete initial object

**Expected output:**
```
Created archive: v1.json.archive
```

**Archive contents (`v1.json.archive`):**
```jsonl
{"version": 1, "created": "2025-01-15T10:00:00Z", "initial": {"id": 1, "name": "Alice", "count": 0, "tags": ["initial"]}}
```

### 2. Append New Observation

```bash
json-archive v1.json.archive v2.json
```

**What happens:**
- Compares v2.json against the current state (v1.json)
- Appends delta changes to the existing archive
- Shows what fields changed between versions

**Expected output:**
```
Appended observation to: v1.json.archive
Changes detected: 3 modifications
```

**New archive contents:**
```jsonl
{"version": 1, "created": "2025-01-15T10:00:00Z", "initial": {"id": 1, "name": "Alice", "count": 0, "tags": ["initial"]}}
["observe", "obs-002", "2025-01-15T10:05:00Z", 3]
["change", "/count", 5, "obs-002"]
["add", "/lastSeen", "2025-01-15", "obs-002"]
["change", "/tags", ["initial", "updated"], "obs-002"]
```

### 3. Continue Building History

```bash
json-archive v1.json.archive v3.json
```

**Expected output:**
```
Appended observation to: v1.json.archive
Changes detected: 4 modifications
```

**Final archive contents:**
```jsonl
{"version": 1, "created": "2025-01-15T10:00:00Z", "initial": {"id": 1, "name": "Alice", "count": 0, "tags": ["initial"]}}
["observe", "obs-002", "2025-01-15T10:05:00Z", 3]
["change", "/count", 5, "obs-002"]
["add", "/lastSeen", "2025-01-15", "obs-002"]
["change", "/tags", ["initial", "updated"], "obs-002"]
["observe", "obs-003", "2025-01-15T10:10:00Z", 4]
["change", "/name", "Bob", "obs-003"]
["change", "/count", 10, "obs-003"]
["change", "/tags", ["updated", "final"], "obs-003"]
["change", "/lastSeen", "2025-01-16", "obs-003"]
["add", "/score", 95, "obs-003"]
```

## Real-World Workflow Example

```bash
# Daily automation script
curl https://example.com/123456/user-profile.json -L -O /tmp/user-profile.json
json-archive \
    /mnt/share/backups/user-profile.json.archive /tmp/user-profile.json \
    --source "my-backup.sh:example.com:123456" \
    --remove-source-files
```

**What you're seeing here:**

1. **Self-documenting archives**: The archive filename `user-profile.json.archive` contains the original filename, making it clear what data is being tracked.

2. **File cleanup with `--remove-source-files`**: This flag moves the JSON file into the archive rather than copying it, automatically cleaning up temporary files in shell scripts.

3. **Flexible file handling**: You don't have to remove the source file. For example, you could snapshot a `state.json` file that some process uses as a database without disrupting the running process.

4. **Source labeling for data integrity**: The `--source` flag creates a unique ID for the JSON object you're tracking. When appending to an existing archive, if the source label doesn't match, the tool refuses to run to protect against data loss. Use your own naming convention to create meaningful identifiers (URLs, script names, etc.).

## Most Useful Features Tour


### 1. Create Archive All At Once

```bash
json-archive v1.json v2.json v3.json
```

**What happens:**
- Creates `v1.json.archive` with all three observations
- Processes each file sequentially, computing deltas between them
- Equivalent to the step-by-step approach above

### 2. Remove Source Files After Archiving

```bash
json-archive v1.json --remove-source-files
```

**What happens:**
- Creates `v1.json.archive`
- **Deletes** `v1.json` after successful archive creation
- Useful for cleanup workflows where you only want the archive

### 3. Force Overwrite Existing Archive

```bash
json-archive --force v1.json
```

**What happens:**
- Overwrites `v1.json.archive` if it already exists
- Without `--force`, the command safely refuses to overwrite

### 4. Custom Output Location

```bash
json-archive -o my-custom.json.archive v1.json v2.json v3.json
```

**What happens:**
- Creates archive at specified path instead of inferring from input filename
- Useful when you want a different naming convention

### 5. Add Source Metadata

```bash
json-archive --source "yt-dlp:youtube:dQw4w9WgXcQ" v1.json v2.json v3.json
```

**What happens:**
- Adds source identifier to archive header
- Helps track where the data came from in complex workflows