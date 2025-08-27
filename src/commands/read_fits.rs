use crate::fits::{format_fits_metadata, read_fits_metadata, FitsMetadata};
use anyhow::Result;
use serde_json;
use std::fs;
use std::path::{Path, PathBuf};

pub fn read_fits(path: &str, verbose: bool, format: &str) -> Result<()> {
    let path = Path::new(path);
    
    if path.is_file() {
        // Single file
        read_single_fits(path, verbose, format)?;
    } else if path.is_dir() {
        // Directory of files
        read_fits_directory(path, verbose, format)?;
    } else {
        return Err(anyhow::anyhow!(
            "Path does not exist or is not accessible: {}",
            path.display()
        ));
    }
    
    Ok(())
}

fn read_single_fits(path: &Path, verbose: bool, format: &str) -> Result<()> {
    let metadata = read_fits_metadata(path)?;
    
    match format.to_lowercase().as_str() {
        "json" => {
            let json_output = if verbose {
                serde_json::to_string_pretty(&metadata)?
            } else {
                // For non-verbose JSON, create a simpler structure
                let simplified = create_simplified_metadata(&metadata);
                serde_json::to_string_pretty(&simplified)?
            };
            println!("{}", json_output);
        },
        "csv" => {
            output_csv_single(&metadata, verbose)?;
        },
        _ => {
            println!("Reading FITS file: {}\n", path.display());
            let formatted = format_fits_metadata(&metadata, verbose);
            println!("{}", formatted);
        }
    }
    
    Ok(())
}

fn read_fits_directory(dir: &Path, verbose: bool, format: &str) -> Result<()> {
    let mut fits_files = Vec::new();
    
    // Recursively find all FITS files
    find_fits_files(dir, &mut fits_files)?;
    
    if fits_files.is_empty() {
        match format.to_lowercase().as_str() {
            "json" => println!("[]"),
            "csv" => {
                // Print CSV header even if no files
                println!("filename,width,height,bit_depth,date_obs,object,exposure,filter,telescope,instrument,gain,ccd_temp,binning,ra,dec,hfr,stars,fwhm");
            }
            _ => println!("No FITS files found in directory."),
        }
        return Ok(());
    }
    
    let mut successful_metadata = Vec::new();
    let mut error_count = 0;
    
    // Read all files and collect metadata
    for file_path in &fits_files {
        match read_fits_metadata(file_path) {
            Ok(metadata) => successful_metadata.push(metadata),
            Err(_) => error_count += 1,
        }
    }
    
    // Output based on format
    match format.to_lowercase().as_str() {
        "json" => {
            let json_output = if verbose {
                serde_json::to_string_pretty(&successful_metadata)?
            } else {
                let simplified: Vec<_> = successful_metadata.iter()
                    .map(create_simplified_metadata)
                    .collect();
                serde_json::to_string_pretty(&simplified)?
            };
            println!("{}", json_output);
        },
        "csv" => {
            output_csv_directory(&successful_metadata, verbose)?;
        },
        _ => {
            println!("Scanning directory: {}\n", dir.display());
            println!("Found {} FITS files\n", fits_files.len());
            
            for (index, metadata) in successful_metadata.iter().enumerate() {
                println!("File {}/{}:", index + 1, fits_files.len());
                let formatted = format_fits_metadata(metadata, verbose);
                println!("{}", formatted);
                
                // Add separator between files if not last
                if index < successful_metadata.len() - 1 {
                    println!("{:-<60}", "");
                }
            }
            
            println!("\nSummary:");
            println!("  Successfully read: {}", successful_metadata.len());
            if error_count > 0 {
                println!("  Errors: {}", error_count);
            }
        }
    }
    
    Ok(())
}

fn find_fits_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir)?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            // Recurse into subdirectories
            find_fits_files(&path, files)?;
        } else if is_fits_file(&path) {
            files.push(path);
        }
    }
    
    Ok(())
}

fn is_fits_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext_lower = ext.to_lowercase();
            ext_lower == "fits" || ext_lower == "fit" || ext_lower == "fts"
        })
        .unwrap_or(false)
}

#[derive(serde::Serialize)]
struct SimplifiedFitsMetadata {
    filename: String,
    width: Option<usize>,
    height: Option<usize>,
    bit_depth: Option<i32>,
    date_obs: Option<String>,
    object: Option<String>,
    exposure: Option<String>,
    filter: Option<String>,
    telescope: Option<String>,
    instrument: Option<String>,
    gain: Option<String>,
    ccd_temp: Option<String>,
    binning: Option<String>,
    ra: Option<String>,
    dec: Option<String>,
    hfr: Option<String>,
    stars: Option<String>,
    fwhm: Option<String>,
}

fn create_simplified_metadata(metadata: &FitsMetadata) -> SimplifiedFitsMetadata {
    SimplifiedFitsMetadata {
        filename: metadata.filename.clone(),
        width: metadata.image_info.as_ref().map(|i| i.width),
        height: metadata.image_info.as_ref().map(|i| i.height),
        bit_depth: metadata.image_info.as_ref().map(|i| i.bit_depth),
        date_obs: metadata.primary_header.get("DATE-OBS").cloned(),
        object: metadata.primary_header.get("OBJECT")
            .or_else(|| metadata.primary_header.get("OBJNAME"))
            .or_else(|| metadata.primary_header.get("TARGET"))
            .cloned(),
        exposure: metadata.primary_header.get("EXPTIME")
            .or_else(|| metadata.primary_header.get("EXPOSURE"))
            .cloned(),
        filter: metadata.primary_header.get("FILTER")
            .or_else(|| metadata.primary_header.get("FILTERNAME"))
            .cloned(),
        telescope: metadata.primary_header.get("TELESCOP").cloned(),
        instrument: metadata.primary_header.get("INSTRUME").cloned(),
        gain: metadata.primary_header.get("GAIN").cloned(),
        ccd_temp: metadata.primary_header.get("CCD-TEMP").cloned(),
        binning: metadata.primary_header.get("XBINNING").cloned(),
        ra: metadata.primary_header.get("OBJCTRA").or(metadata.primary_header.get("RA")).cloned(),
        dec: metadata.primary_header.get("OBJCTDEC").or(metadata.primary_header.get("DEC")).cloned(),
        hfr: metadata.primary_header.get("HFR")
            .or_else(|| metadata.primary_header.get("STARHFR"))
            .or_else(|| metadata.primary_header.get("MEANHFR"))
            .cloned(),
        stars: metadata.primary_header.get("STARS")
            .or_else(|| metadata.primary_header.get("STARCOUNT"))
            .or_else(|| metadata.primary_header.get("NSTARS"))
            .cloned(),
        fwhm: metadata.primary_header.get("STARSFWHM")
            .or_else(|| metadata.primary_header.get("FWHM"))
            .or_else(|| metadata.primary_header.get("MEANFWHM"))
            .cloned(),
    }
}

fn output_csv_single(metadata: &FitsMetadata, verbose: bool) -> Result<()> {
    if verbose {
        // For verbose mode, output all headers as key-value pairs
        println!("filename,key,value");
        println!("{},filename,{}", escape_csv(&metadata.filename), escape_csv(&metadata.filename));
        
        for (key, value) in &metadata.primary_header {
            println!("{},{},{}", escape_csv(&metadata.filename), escape_csv(key), escape_csv(value));
        }
    } else {
        // Standard CSV format
        println!("filename,width,height,bit_depth,date_obs,object,exposure,filter,telescope,instrument,gain,ccd_temp,binning,ra,dec,hfr,stars,fwhm");
        let simplified = create_simplified_metadata(metadata);
        println!("{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            escape_csv(&simplified.filename),
            simplified.width.map(|v| v.to_string()).unwrap_or_default(),
            simplified.height.map(|v| v.to_string()).unwrap_or_default(),
            simplified.bit_depth.map(|v| v.to_string()).unwrap_or_default(),
            escape_csv(&simplified.date_obs.unwrap_or_default()),
            escape_csv(&simplified.object.unwrap_or_default()),
            escape_csv(&simplified.exposure.unwrap_or_default()),
            escape_csv(&simplified.filter.unwrap_or_default()),
            escape_csv(&simplified.telescope.unwrap_or_default()),
            escape_csv(&simplified.instrument.unwrap_or_default()),
            escape_csv(&simplified.gain.unwrap_or_default()),
            escape_csv(&simplified.ccd_temp.unwrap_or_default()),
            escape_csv(&simplified.binning.unwrap_or_default()),
            escape_csv(&simplified.ra.unwrap_or_default()),
            escape_csv(&simplified.dec.unwrap_or_default()),
            escape_csv(&simplified.hfr.unwrap_or_default()),
            escape_csv(&simplified.stars.unwrap_or_default()),
            escape_csv(&simplified.fwhm.unwrap_or_default())
        );
    }
    Ok(())
}

fn output_csv_directory(metadata_list: &[FitsMetadata], verbose: bool) -> Result<()> {
    if verbose {
        // For verbose mode, output all headers as key-value pairs
        println!("filename,key,value");
        for metadata in metadata_list {
            println!("{},filename,{}", escape_csv(&metadata.filename), escape_csv(&metadata.filename));
            for (key, value) in &metadata.primary_header {
                println!("{},{},{}", escape_csv(&metadata.filename), escape_csv(key), escape_csv(value));
            }
        }
    } else {
        // Standard CSV format
        println!("filename,width,height,bit_depth,date_obs,object,exposure,filter,telescope,instrument,gain,ccd_temp,binning,ra,dec,hfr,stars,fwhm");
        for metadata in metadata_list {
            let simplified = create_simplified_metadata(metadata);
            println!("{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                escape_csv(&simplified.filename),
                simplified.width.map(|v| v.to_string()).unwrap_or_default(),
                simplified.height.map(|v| v.to_string()).unwrap_or_default(),
                simplified.bit_depth.map(|v| v.to_string()).unwrap_or_default(),
                escape_csv(&simplified.date_obs.unwrap_or_default()),
                escape_csv(&simplified.object.unwrap_or_default()),
                escape_csv(&simplified.exposure.unwrap_or_default()),
                escape_csv(&simplified.filter.unwrap_or_default()),
                escape_csv(&simplified.telescope.unwrap_or_default()),
                escape_csv(&simplified.instrument.unwrap_or_default()),
                escape_csv(&simplified.gain.unwrap_or_default()),
                escape_csv(&simplified.ccd_temp.unwrap_or_default()),
                escape_csv(&simplified.binning.unwrap_or_default()),
                escape_csv(&simplified.ra.unwrap_or_default()),
                escape_csv(&simplified.dec.unwrap_or_default()),
                escape_csv(&simplified.hfr.unwrap_or_default()),
                escape_csv(&simplified.stars.unwrap_or_default()),
                escape_csv(&simplified.fwhm.unwrap_or_default())
            );
        }
    }
    Ok(())
}

fn escape_csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}