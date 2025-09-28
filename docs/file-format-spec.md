# JSON Archive Format Specification v1.0

## Overview

A JSONL (JSON Lines) format for tracking the evolution of JSON objects over time using delta-based changes with JSON Pointer (RFC 6901) for path notation. Inspired by asciicinema v3 format.

## File Structure

```
{header}
[event]
[event]
...
```

- First line: JSON header object
- Following lines: Event arrays or comments
- File extension: `.json.archive`
- Encoding: UTF-8

### File Extension Rationale

The `.json.archive` extension is designed for workflows where a command line program generates the same `filename.json` file daily. By appending `.archive` to the existing filename, minimal changes are needed to existing processes.

For example, if you have a daily process that generates `data.json`, the archive becomes `data.json.archive`. This makes shell pipeline operations intuitive:

```bash
json-archive concat data.json.archive data.json --remove-source-files
```

This command clearly shows that `json-archive` is moving the contents of `data.json` into the archive, making the operation self-documenting and requiring minimal changes to existing workflows.

## Header Format

```json
{
  "version": 1,
  "created": "ISO-8601 timestamp",
  "source": "optional source identifier",
  "initial": { ... initial object state ... },
  "metadata": { ... optional metadata ... }
}
```

Required fields:
- `version`: Format version (currently 1)
- `created`: ISO-8601 timestamp of archive creation
- `initial`: Complete initial state of the tracked object

## Event Types

Each event is a JSON array with the event type as the first element.

### 1. Observe Event
Marks the beginning of an observation with a count of following changes.

```json
["observe", observationId, timestamp, changeCount]
```

- `observationId`: Unique string identifier (can be any string: UUID, timestamp, sequential ID, etc.)
- `timestamp`: ISO-8601 timestamp
- `changeCount`: Number of add/change/remove/move events that follow

### 2. Add Event
Adds a new field to the object.

```json
["add", path, value, observationId]
```

- `path`: JSON Pointer path to the field
- `value`: Any JSON value
- `observationId`: String referencing the preceding observe event

### 3. Change Event
Modifies an existing field value.

```json
["change", path, newValue, observationId]
```

- `path`: JSON Pointer path to the field
- `newValue`: New value
- `observationId`: String referencing the preceding observe event

### 4. Remove Event
Removes a field from the object.

```json
["remove", path, observationId]
```

- `path`: JSON Pointer path to the field
- `observationId`: String referencing the preceding observe event

### 5. Move Event
Reorders existing elements within an array. Moves are applied sequentially.

```json
["move", path, [[fromIndex, toIndex], ...], observationId]
```

- `path`: JSON Pointer path to the array
- `[[fromIndex, toIndex], ...]`: List of move operations applied in order
- `observationId`: String referencing the preceding observe event

**Important implementation detail:** Each move operation should:
1. First, insert a copy of the element at `fromIndex` into position `toIndex`
2. Then, remove the original element from its position (accounting for any shift caused by the insertion)

This approach prevents index calculation errors. When `fromIndex > toIndex`, the removal happens at `fromIndex + 1`. When `fromIndex < toIndex`, the removal happens at `fromIndex`.

Example: Given array [A, B, C, D]:
```json
["move", "/items", [[3, 1]], "obs-001"]
```
Step 1: Insert D at index 1 → [A, D, B, C, D]
Step 2: Remove from index 4 → [A, D, B, C]

For multiple moves on [A, B, C]:
```json
["move", "/items", [[2, 0], [2, 1]], "obs-001"]
```
First move: Insert C at 0 → [C, A, B, C], remove from 3 → [C, A, B]
Second move: Insert B at 1 → [C, B, A, B], remove from 3 → [C, B, A]

Note: Use `add` events for new elements, not `move`. Move is strictly for reordering existing elements.

### 6. Snapshot Event
Stores complete object state for faster seeking and append performance.

```json
["snapshot", observationId, timestamp, object]
```

- `observationId`: Unique string identifier for this snapshot
- `timestamp`: ISO-8601 timestamp
- `object`: Complete object state at this point

Snapshot events are interchangeable with observe+delta sequences: you can rewrite
["observe", observatinID, timestamp, N] followed by N delta events into a single
[ "snapshot", observationID, timestamp, object], and vice versa. 

Implementation Reality: The current tool naively appends snapshots without removing
equivalent observe+delta sequences. Yhis is lazy programming for implementation ease.
Better tools should replace the delta sequence with the snapshot, not duplicate
the information.

The snapshot placement strategy is about append performance optimization.

1. **Append requires replay:** To append a new observation, you must know the current
state. The tool seeks backward from EOF to find the most recent snapshot, then
replays forward from there. Placing snapshots near the end of large archives
minimizes this replay cost.
2. **Snapshot placement is arbitrary:** Unlike fixed-interval I-frames in video codecs,
snapshots can be placed anywhere based on access patterns and file size.
by moving/adding snapshots closer to the end, reducing replay cost on append.
Archives can be "losslessly re-encoded" by repositioning snapshots based on
access patterns and file size.
3. Practical example:
    - Small files (10 versions): No snapshots needed, full replay is cheap
    - Large files (GBs of updates): Place snapshots near end-of-file so append only replays
      recent deltas
    - High-frequency appends: Consider periodic re-encoding to maintain snapshots near EOF

## Identifier Format

The `observationId` field used throughout events is an arbitrary string that must be unique within the file. Common patterns include:
- UUIDs: `"550e8400-e29b-41d4-a716-446655440000"`
- Timestamps: `"1705325400.123"`
- Sequential IDs: `"obs-001"`, `"obs-002"`
- ISO timestamps: `"2025-01-15T10:05:00.123Z"`
- Any other string scheme that guarantees uniqueness

## Path Notation

Paths use JSON Pointer notation (RFC 6901):

- Object fields: `/user/profile/name`
- Array elements: `/items/0`
- Root level: `/fieldname`
- Empty string key: `/`
- Escape sequences: `~0` for `~`, `~1` for `/`

Examples:
- `/users/0/email` - First user's email
- `/metadata/tags/3` - Fourth tag in tags array
- `/strange~1key` - Key containing forward slash

## Comments

Lines beginning with `#` are treated as comments and ignored by parsers.

```
# This is a comment
["observe", "obs-001", "2025-01-15T10:00:00Z", 1]
```

## Example File

```json
{"version": 1, "created": "2025-01-15T10:00:00Z", "initial": {"id": 1, "views": 0, "tags": ["api", "v1"]}}
# First observation - using sequential ID
["observe", "obs-001", "2025-01-15T10:05:00Z", 2]
["add", "/title", "Hello World", "obs-001"]
["change", "/views", 10, "obs-001"]
# Array modification - using UUID
["observe", "550e8400-e29b-41d4-a716-446655440000", "2025-01-15T10:10:00Z", 3]
["change", "/views", 25, "550e8400-e29b-41d4-a716-446655440000"]
["add", "/tags/2", "public", "550e8400-e29b-41d4-a716-446655440000"]
["move", "/tags", [[2, 0]], "550e8400-e29b-41d4-a716-446655440000"]
# Snapshot - using timestamp as ID
["snapshot", "1705325700.456", "2025-01-15T10:15:00Z", {"id": 1, "views": 25, "title": "Hello World", "tags": ["public", "api", "v1"]}]
["observe", "obs-003", "2025-01-15T10:20:00Z", 2]
["add", "/likes", 5, "obs-003"]
["remove", "/title", "obs-003"]
```

## Reading Algorithm

1. Parse header from first line
2. Initialize state from `header.initial`
3. For each subsequent line:
   - Skip if comment (`#`)
   - Parse JSON array
   - Apply event based on type:
     - `observe`: Note changeCount for bounded reading
     - `add`: Set field at path
     - `change`: Update field at path
     - `remove`: Delete field at path
     - `move`: Apply array reordering operations sequentially
     - `snapshot`: Optionally update state completely

**Important:** Observations in the archive file are not required to be in chronological order. The reader implementation should parse all events and sort them by timestamp if chronological ordering is needed for the use case.

## CLI Implementation Notes

### Basic Command Structure
```bash
jsonarchive create output.jarch input1.json input2.json ...
```

### Processing Logic
1. Read first JSON file as initial state
2. For each subsequent JSON file:
   - Compare with current state
   - Generate observe event with timestamp
   - Generate add/change/remove/move events for differences (using JSON Pointer paths)
   - Update current state
3. Optionally insert snapshots based on:
   - Number of observations (e.g., every 100)
   - Size of accumulated deltas
   - Time intervals

### JSON Pointer Implementation
- Implement RFC 6901 compliant JSON Pointer resolution
- Handle escape sequences: `~0` → `~`, `~1` → `/`
- Validate paths before applying operations
- Array indices must be valid integers

### Diff Generation

**For objects:**
1. Recursively traverse both objects
2. For keys in new but not old: generate `add`
3. For keys in old but not new: generate `remove`
4. For keys in both with different values: generate `change`

**For arrays:**
1. Identify common elements (present in both arrays)
2. Generate `remove` for elements only in old array
3. Generate `add` for elements only in new array
4. Generate `change` for common elements with different values
5. Generate minimal `move` sequence for common elements in different positions

**Important:** New array elements should always use `add` operations at their final positions. The `move` operation is strictly for reordering existing elements. This keeps the semantics clear and the diff minimal.

## Design Rationale

- **Delta-based**: Minimizes file size for small incremental changes
- **Self-contained**: Header contains initial state, making file complete
- **Human-readable**: JSONL with comments for debugging
- **Standards-compliant**: Uses RFC 6901 JSON Pointer for path notation
- **Seekable**: Snapshots allow jumping to recent states without full replay
- **Simple parsing**: Line-based format with standard JSON
- **Change bounds**: Observe events include count for predictable reads

