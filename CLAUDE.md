# CLAUDE.md - Development Notes

## Project Overview

TS-Screen (N.I.N.A Target Scheduler Plugin File Screener) is a Rust CLI utility designed to analyze N.I.N.A. Target Scheduler plugin databases and manage rejected astronomical image files. The project was developed to help organize FITS files based on their grading status in the N.I.N.A. Target Scheduler database.

## Key Implementation Details

### Architecture

The application uses a command-pattern architecture with the following main components:

1. **CLI Interface**: Built with clap-derive for type-safe command parsing
2. **Database Layer**: Uses rusqlite for SQLite database access
3. **File Operations**: Standard library fs module for file system operations
4. **Data Models**: Serialize/Deserialize structs for Project, Target, and AcquiredImage

### Security Considerations

- **SQL Injection Prevention**: All database queries use parameterized statements
- **Safe File Operations**: Explicit dry-run mode before any destructive operations
- **Path Validation**: Careful handling of file paths from database to prevent directory traversal

### Complex Implementation Areas

#### 1. Multi-Structure Path Detection

The most complex part is handling different directory structures. The code supports:

```rust
// Standard: date/target_name/date/LIGHT/filename
// Alternate: target_name/date/LIGHT/filename
```

The implementation:
- First attempts standard structure parsing
- Falls back to alternate structure if files not found
- Uses date pattern detection (YYYY-MM-DD) to identify structure
- Handles files in both `LIGHT/` and `LIGHT/rejected/` subdirectories

#### 2. Metadata Extraction

Image filenames are extracted from JSON metadata stored in the database:
```rust
fn extract_filename(metadata: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(metadata).ok()?;
    json.get("FileName")
        .and_then(|f| f.as_str())
        .map(|path| {
            // Extract just the filename from the full path
            path.split(&['\\', '/'][..])
                .last()
                .unwrap_or(path)
                .to_string()
        })
}
```

#### 3. Column Name Handling

The database uses inconsistent column naming (camelCase vs snake_case). The code handles this by using exact column names from the schema:
- `Id`, `projectId`, `targetId` (not `id`, `project_id`, etc.)
- `acquireddate`, `filtername` (not `acquired_date`, `filter_name`)

### Testing Strategy

Testing focused on:
1. **Dry-run verification**: Always test with `--dry-run` first
2. **SQL injection attempts**: Verified parameterized queries work correctly
3. **Multiple directory structures**: Tested both `files/` and `files2/` directories
4. **Edge cases**: Files already in rejected folders, missing files, etc.

### Performance Considerations

- Batch operations where possible (e.g., reading all rejected files in one query)
- Minimal file system checks (only when necessary)
- Efficient path parsing without regex where possible

### Recent Updates (2025-08-27)

#### Statistical Grading Enhancement
- Refactored from filter-based to target-and-filter-based analysis
- Added cloud detection algorithm with sequence analysis
- Implemented rolling baseline establishment after cloud events

#### Key Implementation Details

1. **Data Flow**:
   - Images grouped by `(target_id, filter_name)` tuples
   - Sorted chronologically within each group for sequence analysis
   - Per-target statistics calculated independently

2. **Cloud Detection Algorithm**:
   - Rolling median baseline (default: 5 images)
   - Triggers on 20% change (configurable)
   - Requires new baseline after cloud event
   - Dual detection: HFR increase OR star count decrease

3. **Statistical Methods**:
   - Standard deviation for normal distributions
   - MAD (Median Absolute Deviation) for skewed distributions
   - Automatic detection of distribution type based on median/mean difference

### Future Improvements

1. **Parallel Processing**: File moves could be parallelized for large batches
2. **Progress Bars**: For long-running operations
3. **Undo Capability**: Track moves in a local database for reversal
4. **Configuration File**: Support for .tsscreenrc configuration
5. **Extended Filtering**: More complex queries (date ranges, multiple statuses)
6. **Machine Learning**: Train models on accepted/rejected images for better detection
7. **Real-time Monitoring**: Watch mode for live sessions

### Statistical Grading Module (grading.rs)

The statistical grading module provides advanced outlier detection:

#### Key Structures
- `StatisticalGradingConfig`: Configuration for all analysis features
- `ImageStatistics`: Per-image metrics (HFR, star count, target info)
- `FilterStatistics`: Aggregate statistics per target/filter group
- `StatisticalRejection`: Rejection details with reason and explanation

#### Algorithm Flow
1. Parse metadata to extract HFR and star counts
2. Group images by (target_id, filter_name)
3. Sort chronologically for sequence analysis
4. Calculate distribution statistics
5. Apply multiple detection methods:
   - Z-score for normal distributions
   - MAD for skewed distributions
   - Sequence analysis for cloud detection

#### Edge Cases Handled
- Images with missing HFR/star count data
- Groups with insufficient data (< 3 images)
- Extreme outliers (0 HFR values)
- Baseline reset after cloud events

### Development Commands

```bash
# Run with verbose logging
RUST_LOG=debug cargo run -- filter-rejected db.sqlite files --dry-run

# Test statistical grading
cargo run -- filter-rejected schedulerdb.sqlite files --dry-run \
  --enable-statistical --stat-hfr --stat-stars --stat-clouds

# Run tests
cargo test

# Check for common issues
cargo clippy

# Format code
cargo fmt
```

### Known Issues

1. **Path Separator Handling**: The code handles both Windows (`\`) and Unix (`/`) paths, but mixed paths might cause issues
2. **Large Metadata**: Very large metadata JSON could cause memory issues (unlikely with typical FITS metadata)
3. **Timezone Handling**: Dates are stored as Unix timestamps, timezone conversion not implemented

### Database Schema Notes

Key relationships:
- `project` (1:many) -> `target`
- `target` (1:many) -> `acquiredimage`
- `exposureplan` links to both `target` and `exposuretemplate`

Critical fields:
- `acquiredimage.gradingStatus`: 0=Pending, 1=Accepted, 2=Rejected
- `acquiredimage.metadata`: JSON containing file path and imaging parameters
- `acquiredimage.rejectreason`: Human-readable rejection reason

### Error Handling Philosophy

The code uses `anyhow` for error handling with context:
- File operations provide specific paths in errors
- Database errors include the operation being attempted
- User-friendly messages for common issues (file not found, permission denied)

### Code Style Notes

- Prefer explicit types in complex sections for clarity
- Use descriptive variable names even if longer
- Comment complex logic, especially path parsing
- Keep functions focused on single responsibilities