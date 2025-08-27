# PSF Guard

[![CI](https://github.com/theatrus/psf-guard/actions/workflows/ci.yml/badge.svg)](https://github.com/theatrus/psf-guard/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

A Rust utility for analyzing N.I.N.A. (Nighttime Imaging 'N' Astronomy) Target Scheduler plugin databases and managing astronomical image files. While designed to work with the Target Scheduler plugin, many features (like FITS metadata reading) can be used independently of the Target Scheduler.

## Documentation

- [Statistical Grading Guide](STATISTICAL_GRADING.md) - Detailed documentation on statistical analysis features
- [Development Notes](CLAUDE.md) - Technical implementation details for developers

## Overview

PSF Guard provides tools to:
- **Target Scheduler Integration**: Query and analyze image grading results from N.I.N.A. Target Scheduler SQLite databases
- **Project Management**: List projects and targets with their imaging statistics
- **File Organization**: Filter and organize rejected image files based on database grading status
- **FITS Analysis**: Read and display metadata from FITS astronomical image files (works independently)
- **Statistical Grading**: Advanced outlier detection using HFR, star count, and cloud detection algorithms
- **Multiple Formats**: Support for JSON, CSV, and table output formats
- **Directory Support**: Handle multiple directory structures for image organization

## Installation

### Prerequisites
- Rust 1.89.0 (see rust-toolchain.toml)
- SQLite3

### Building from Source
```bash
git clone https://github.com/theatrus/psf-guard.git
cd psf-guard
cargo build --release
```

The compiled binary will be available at `target/release/psf-guard`.

## Usage

### Basic Commands

#### List all projects
```bash
psf-guard list-projects
```

#### List targets for a specific project
```bash
psf-guard list-targets "Project Name"
```

#### Dump grading results
```bash
# Show all images
psf-guard dump-grading

# Filter by status (pending, accepted, rejected)
psf-guard dump-grading --status rejected

# Filter by project
psf-guard dump-grading --project "Cygnus Wall"

# Filter by target
psf-guard dump-grading --target "North American"

# Output formats (table, json, csv)
psf-guard dump-grading --format json
```

### Read FITS File Metadata

Display metadata from FITS astronomical image files:

```bash
# Read a single FITS file
psf-guard read-fits image.fits

# Read all FITS files in a directory (recursive)
psf-guard read-fits /path/to/fits/directory

# Show all header keywords (verbose mode)
psf-guard read-fits --verbose image.fits

# Output formats (table, json, csv)
psf-guard read-fits --format json image.fits
psf-guard read-fits --format csv /path/to/fits/directory
```

### Filter Rejected Files

The `filter-rejected` command moves rejected image files to a `LIGHT_REJECT` directory based on database grading status.

**IMPORTANT: Always use `--dry-run` first to preview what will be moved!**

```bash
# Dry run (recommended first step)
psf-guard filter-rejected schedulerdb.sqlite /path/to/images --dry-run

# Filter by project
psf-guard filter-rejected schedulerdb.sqlite /path/to/images --dry-run --project "Double Dragon"

# Actually move files (use with caution!)
psf-guard filter-rejected schedulerdb.sqlite /path/to/images --project "Double Dragon"
```

### Supported Directory Structures

The utility automatically detects and handles two common directory structures:

1. **Standard Structure**: `date/target_name/date/LIGHT/filename.fits`
   ```
   files/
   └── 2025-08-25/
       └── Sh2 157/
           └── 2025-08-25/
               ├── LIGHT/
               │   └── image.fits
               └── LIGHT_REJECT/  (created by utility)
                   └── image.fits
   ```

2. **Alternate Structure**: `target_name/date/LIGHT/filename.fits`
   ```
   files2/
   └── Bubble Nebula/
       └── 2025-08-17/
           ├── LIGHT/
           │   └── image.fits
           └── LIGHT_REJECT/  (created by utility)
               └── image.fits
   ```

The utility also handles files already in `LIGHT/rejected/` subdirectories and moves them to the appropriate `LIGHT_REJECT/` directory.

### Statistical Grading

Beyond the database grading status, PSF Guard can perform statistical analysis to identify additional outliers:

- **HFR Analysis**: Detects images with Half Flux Radius (focus quality) significantly different from the target's distribution
- **Star Count Analysis**: Identifies images with abnormal star detection counts per target
- **Distribution Analysis**: Uses Median Absolute Deviation (MAD) for skewed distributions where median differs significantly from mean
- **Cloud Detection**: Monitors sequences for sudden rises in HFR or drops in star count that indicate clouds, then waits for stable baseline before accepting images again

Statistical grading analyzes all images per target and filter combination to establish baselines, then identifies outliers that may have been missed by the initial grading process. The analysis is target-specific to account for different imaging conditions across the sky.

For detailed information about statistical grading features, algorithms, and best practices, see [STATISTICAL_GRADING.md](STATISTICAL_GRADING.md).

## Database Schema

The utility expects a SQLite database with the following key tables:
- `project`: Contains project information
- `target`: Contains observation targets
- `acquiredimage`: Contains image metadata and grading status

Grading status values:
- 0 = Pending
- 1 = Accepted
- 2 = Rejected

## Command Reference

### Global Options
- `-d, --database <DATABASE>`: Database file to use (default: schedulerdb.sqlite)

### Commands

#### dump-grading
Dump grading results for all images

Options:
- `-s, --status <STATUS>`: Filter by grading status (pending, accepted, rejected)
- `-p, --project <PROJECT>`: Filter by project name (partial match)
- `-t, --target <TARGET>`: Filter by target name (partial match)
- `-f, --format <FORMAT>`: Output format (table, json, csv) [default: table]

#### list-projects
List all projects in the database

#### list-targets
List all targets for a specific project

Arguments:
- `<PROJECT>`: Project ID or name

#### filter-rejected
Filter rejected files and move them to LIGHT_REJECT folders

Arguments:
- `<DATABASE>`: Database file to use
- `<BASE_DIR>`: Base directory containing the image files

Options:
- `--dry-run`: Perform a dry run (show what would be moved without actually moving)
- `-p, --project <PROJECT>`: Filter by project name
- `-t, --target <TARGET>`: Filter by target name
- `--enable-statistical`: Enable statistical analysis for additional rejections
- `--stat-hfr`: Enable HFR outlier detection
- `--hfr-stddev <STDDEV>`: Standard deviations for HFR outlier detection (default: 2.0)
- `--stat-stars`: Enable star count outlier detection  
- `--star-stddev <STDDEV>`: Standard deviations for star count outlier detection (default: 2.0)
- `--stat-distribution`: Enable distribution analysis (median/mean shift detection)
- `--median-shift-threshold <THRESHOLD>`: Percentage threshold for median shift from mean (default: 0.1)
- `--stat-clouds`: Enable cloud detection (sudden rises in HFR or drops in star count)
- `--cloud-threshold <THRESHOLD>`: Percentage threshold for cloud detection (default: 0.2 = 20% change)
- `--cloud-baseline-count <COUNT>`: Number of images needed to establish baseline after cloud event (default: 5)

#### read-fits
Read and display metadata from FITS files

Arguments:
- `<PATH>`: Path to FITS file or directory containing FITS files

Options:
- `-v, --verbose`: Show verbose output with all headers
- `-f, --format <FORMAT>`: Output format (table, json, csv) [default: table]

#### regrade
Regrade images in the database based on statistical analysis

Arguments:
- `<DATABASE>`: Database file to use

Options:
- `--dry-run`: Perform a dry run (show what would be changed without actually updating)
- `-p, --project <PROJECT>`: Filter by project name
- `-t, --target <TARGET>`: Filter by target name
- `--days <DAYS>`: Number of days to look back (default: 90)
- `--reset <MODE>`: Reset mode: none, automatic, or all (default: none)
  - `none`: Do not reset any existing grades
  - `automatic`: Reset only automatically graded images (preserves manual grades)
  - `all`: Reset all images to pending status
- Statistical analysis options (same as filter-rejected command)

## Safety Features

1. **Parameterized Queries**: All database queries use parameterized statements to prevent SQL injection
2. **Dry Run Mode**: The `--dry-run` flag shows what would be changed without making actual modifications
3. **Explicit Arguments**: Critical operations require explicit database and directory arguments
4. **Detailed Output**: Shows source and destination paths, rejection reasons, and operation summaries

## Examples

```bash
# Check what rejected files exist for a project
psf-guard dump-grading --status rejected --project "Double Dragon" --format csv > rejected_files.csv

# Preview file moves for a specific project
psf-guard filter-rejected mydb.sqlite ./images --dry-run --project "Cygnus Wall"

# Move all rejected files for a target
psf-guard filter-rejected mydb.sqlite ./images --target "LDN 1228"

# Get JSON output for integration with other tools
psf-guard dump-grading --status accepted --format json | jq '.[] | select(.filter_name == "HA")'

# Use statistical grading to find outliers beyond database rejections
psf-guard filter-rejected mydb.sqlite ./images --dry-run --enable-statistical --stat-hfr --stat-stars

# Fine-tune statistical thresholds
psf-guard filter-rejected mydb.sqlite ./images --dry-run --enable-statistical --stat-hfr --hfr-stddev 1.5 --stat-distribution --median-shift-threshold 0.15

# Enable cloud detection with custom thresholds
psf-guard filter-rejected mydb.sqlite ./images --dry-run --enable-statistical --stat-clouds --cloud-threshold 0.15 --cloud-baseline-count 3

# Regrade images in database (last 30 days)
psf-guard regrade mydb.sqlite --dry-run --days 30 --enable-statistical --stat-hfr --stat-stars

# Reset automatic grades and reapply statistical analysis
psf-guard regrade mydb.sqlite --dry-run --reset automatic --enable-statistical --stat-hfr --stat-stars --stat-clouds

# Reset all grades for a specific target
psf-guard regrade mydb.sqlite --dry-run --reset all --target "M31" --days 7

# Analyze FITS file metadata
psf-guard read-fits "image.fits"

# Check all FITS files in a directory
psf-guard read-fits "/path/to/fits/files/"

# Show all header keywords for debugging
psf-guard read-fits --verbose "image.fits"

# Export FITS metadata to JSON or CSV for analysis
psf-guard read-fits --format json "/path/to/fits/files/" > metadata.json
psf-guard read-fits --format csv "/path/to/fits/files/" > metadata.csv
```

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.