use anyhow::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Metadata extracted from a FITS file
#[derive(Debug, serde::Serialize)]
pub struct FitsMetadata {
    pub filename: String,
    pub headers: Vec<HeaderInfo>,
    pub primary_header: HashMap<String, String>,
    pub image_info: Option<ImageInfo>,
}

#[derive(Debug, serde::Serialize)]
pub struct HeaderInfo {
    pub hdu_index: usize,
    pub hdu_name: Option<String>,
    pub keywords: HashMap<String, String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ImageInfo {
    pub width: usize,
    pub height: usize,
    pub bit_depth: i32,
    pub dimensions: Vec<usize>,
}

/// Read metadata from a FITS file using basic FITS format parsing
pub fn read_fits_metadata(path: &Path) -> Result<FitsMetadata> {
    let mut file = File::open(path)?;

    // Read header blocks until we find END
    let mut header_data = Vec::new();
    loop {
        let mut block = vec![0u8; 2880];
        match file.read_exact(&mut block) {
            Ok(_) => {
                // Check if this block contains the END keyword
                let block_str = String::from_utf8_lossy(&block);
                header_data.extend_from_slice(&block);
                if block_str.contains("END ") {
                    break;
                }
            }
            Err(_) => break, // End of file
        }

        // Safety limit - don't read more than 10 blocks (28.8 KB)
        if header_data.len() > 28800 {
            break;
        }
    }

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Parse the primary header
    let keywords = parse_fits_header(&header_data)?;
    let primary_header = keywords.clone();

    // Extract image info from primary header
    let mut image_info = None;
    if let Some(naxis) = keywords.get("NAXIS").and_then(|s| s.parse::<usize>().ok()) {
        if naxis > 0 {
            let mut dimensions = Vec::new();
            for i in 1..=naxis {
                if let Some(dim) = keywords
                    .get(&format!("NAXIS{}", i))
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    dimensions.push(dim);
                }
            }

            if dimensions.len() >= 2 {
                let width = dimensions[0];
                let height = dimensions[1];
                let bit_depth = keywords
                    .get("BITPIX")
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(0);

                image_info = Some(ImageInfo {
                    width,
                    height,
                    bit_depth,
                    dimensions,
                });
            }
        }
    }

    let headers = vec![HeaderInfo {
        hdu_index: 0,
        hdu_name: keywords.get("EXTNAME").cloned(),
        keywords,
    }];

    Ok(FitsMetadata {
        filename,
        headers,
        primary_header,
        image_info,
    })
}

/// Parse a FITS header block (2880 bytes)
fn parse_fits_header(data: &[u8]) -> Result<HashMap<String, String>> {
    let mut keywords = HashMap::new();

    // FITS headers are organized in 80-character cards
    for chunk in data.chunks(80) {
        if let Ok(card) = std::str::from_utf8(chunk) {
            let card = card.trim();

            // Check for END keyword
            if card.starts_with("END") {
                break;
            }

            // Skip empty cards, COMMENT, and HISTORY
            if card.is_empty() || card.starts_with("COMMENT") || card.starts_with("HISTORY") {
                continue;
            }

            // Parse KEYWORD = VALUE / COMMENT format
            if let Some(eq_pos) = card.find('=') {
                let keyword = card[..eq_pos].trim();
                let value_part = &card[eq_pos + 1..];

                // Find the value (before the comment if any)
                let value = if let Some(comment_pos) = value_part.find('/') {
                    value_part[..comment_pos].trim()
                } else {
                    value_part.trim()
                };

                // Clean up the value (remove quotes if present)
                let cleaned_value = value
                    .trim_matches('\'')
                    .trim_matches('"')
                    .trim()
                    .to_string();

                if !keyword.is_empty() {
                    keywords.insert(keyword.to_string(), cleaned_value);
                }
            }
        }
    }

    Ok(keywords)
}

/// Format FITS metadata for display
pub fn format_fits_metadata(metadata: &FitsMetadata, verbose: bool) -> String {
    let mut output = String::new();

    // File information
    output.push_str(&format!("FITS File: {}\n", metadata.filename));
    output.push_str(&format!("HDUs: {}\n", metadata.headers.len()));

    // Image information
    if let Some(ref img_info) = metadata.image_info {
        output.push_str("\nImage Information:\n");
        output.push_str(&format!(
            "  Dimensions: {} x {}\n",
            img_info.width, img_info.height
        ));
        output.push_str(&format!("  Bit Depth: {}\n", img_info.bit_depth));
        if img_info.dimensions.len() > 2 {
            output.push_str(&format!("  Full Shape: {:?}\n", img_info.dimensions));
        }
    }

    // Key metadata from primary header
    output.push_str("\nKey Metadata:\n");

    // Observation info
    if let Some(date_obs) = metadata.primary_header.get("DATE-OBS") {
        output.push_str(&format!("  Date: {}\n", date_obs));
    }

    // Try to get object name
    if let Some(object) = metadata
        .primary_header
        .get("OBJECT")
        .or_else(|| metadata.primary_header.get("OBJNAME"))
        .or_else(|| metadata.primary_header.get("TARGET"))
    {
        output.push_str(&format!("  Object: {}\n", object));
    }

    if let Some(exptime) = metadata
        .primary_header
        .get("EXPTIME")
        .or_else(|| metadata.primary_header.get("EXPOSURE"))
    {
        output.push_str(&format!("  Exposure: {}s\n", exptime));
    }

    // Filter information from FITS headers only
    if let Some(filter) = metadata
        .primary_header
        .get("FILTER")
        .or_else(|| metadata.primary_header.get("FILTERNAME"))
    {
        output.push_str(&format!("  Filter: {}\n", filter));
    }

    // Equipment info
    if let Some(telescope) = metadata.primary_header.get("TELESCOP") {
        output.push_str(&format!("  Telescope: {}\n", telescope));
    }
    if let Some(instrument) = metadata.primary_header.get("INSTRUME") {
        output.push_str(&format!("  Instrument: {}\n", instrument));
    }

    // Image quality info (try multiple keyword variations)
    if let Some(hfr) = metadata
        .primary_header
        .get("HFR")
        .or_else(|| metadata.primary_header.get("STARHFR"))
        .or_else(|| metadata.primary_header.get("MEANHFR"))
    {
        output.push_str(&format!("  HFR: {}\n", hfr));
    }
    if let Some(stars) = metadata
        .primary_header
        .get("STARS")
        .or_else(|| metadata.primary_header.get("STARCOUNT"))
        .or_else(|| metadata.primary_header.get("NSTARS"))
    {
        output.push_str(&format!("  Stars: {}\n", stars));
    }
    if let Some(fwhm) = metadata
        .primary_header
        .get("STARSFWHM")
        .or_else(|| metadata.primary_header.get("FWHM"))
        .or_else(|| metadata.primary_header.get("MEANFWHM"))
    {
        output.push_str(&format!("  FWHM: {}\n", fwhm));
    }

    // Additional image statistics if available
    if let Some(median) = metadata
        .primary_header
        .get("MEDIAN")
        .or_else(|| metadata.primary_header.get("MEDIANPX"))
    {
        output.push_str(&format!("  Median: {}\n", median));
    }
    if let Some(mean) = metadata
        .primary_header
        .get("MEAN")
        .or_else(|| metadata.primary_header.get("MEANPX"))
    {
        output.push_str(&format!("  Mean: {}\n", mean));
    }
    if let Some(stddev) = metadata
        .primary_header
        .get("STDEV")
        .or_else(|| metadata.primary_header.get("STDDEV"))
    {
        output.push_str(&format!("  StdDev: {}\n", stddev));
    }

    // Camera settings
    if let Some(gain) = metadata.primary_header.get("GAIN") {
        output.push_str(&format!("  Gain: {}\n", gain));
    }
    if let Some(temp) = metadata.primary_header.get("CCD-TEMP") {
        output.push_str(&format!("  CCD Temp: {}°C\n", temp));
    }
    if let Some(binning) = metadata.primary_header.get("XBINNING") {
        output.push_str(&format!("  Binning: {}x{}\n", binning, binning));
    }

    // Position info
    if let Some(ra) = metadata
        .primary_header
        .get("OBJCTRA")
        .or(metadata.primary_header.get("RA"))
    {
        if let Some(dec) = metadata
            .primary_header
            .get("OBJCTDEC")
            .or(metadata.primary_header.get("DEC"))
        {
            output.push_str(&format!("  RA/Dec: {} / {}\n", ra, dec));
        }
    }

    // If verbose, show all headers
    if verbose {
        for header in &metadata.headers {
            output.push_str(&format!("\nHDU {} ", header.hdu_index));
            if let Some(ref name) = header.hdu_name {
                output.push_str(&format!("({})", name));
            }
            output.push_str(" - All Keywords:\n");

            let mut sorted_keys: Vec<_> = header.keywords.iter().collect();
            sorted_keys.sort_by_key(|&(k, _)| k);

            for (key, value) in sorted_keys {
                output.push_str(&format!("  {:<16} = {}\n", key, value));
            }
        }
    }

    output
}
