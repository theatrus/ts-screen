use crate::db::Database;
use anyhow::{Context, Result};
use crate::hocus_focus_star_detection::{detect_stars_hocus_focus, HocusFocusParams};
use crate::image_analysis::{FitsImage, ImageStatistics as ComputedStats};
use crate::mtf_stretch::{stretch_image, StretchParameters};
use crate::nina_star_detection::{
    detect_stars_with_original, NoiseReduction, StarDetectionParams, StarSensitivity,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub fn analyze_fits_and_compare(
    conn: &Connection,
    fits_path: &str,
    project_filter: Option<String>,
    target_filter: Option<String>,
    format: &str,
    detector: &str,
    sensitivity: &str,
    apply_stretch: bool,
    _verbose: bool,
) -> Result<()> {
    let db = Database::new(conn);
    let fits_path = Path::new(fits_path);

    if fits_path.is_file() {
        analyze_single_fits(
            &db,
            fits_path,
            project_filter,
            target_filter,
            format,
            detector,
            sensitivity,
            apply_stretch,
        )?;
    } else if fits_path.is_dir() {
        analyze_fits_directory(
            &db,
            fits_path,
            project_filter,
            target_filter,
            format,
            detector,
            sensitivity,
            apply_stretch,
        )?;
    } else {
        return Err(anyhow::anyhow!(
            "Path does not exist or is not accessible: {}",
            fits_path.display()
        ));
    }

    Ok(())
}

fn analyze_single_fits(
    db: &Database,
    fits_path: &Path,
    project_filter: Option<String>,
    target_filter: Option<String>,
    format: &str,
    detector: &str,
    sensitivity: &str,
    apply_stretch: bool,
) -> Result<()> {
    let filename = fits_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?;

    println!("Analyzing FITS file: {}", fits_path.display());

    // Load and analyze the FITS image
    let image = FitsImage::from_file(fits_path)
        .with_context(|| format!("Failed to load FITS image: {}", fits_path.display()))?;

    let mut computed_stats = image.calculate_statistics();

    // Perform star detection
    perform_star_detection(
        &image,
        &mut computed_stats,
        detector,
        sensitivity,
        apply_stretch,
    )?;

    // Try to find corresponding database entry
    let db_entries = find_database_entry(db, filename, project_filter, target_filter)?;

    match format.to_lowercase().as_str() {
        "json" => output_json_comparison(&computed_stats, &db_entries, filename)?,
        "csv" => output_csv_comparison(&computed_stats, &db_entries, filename)?,
        _ => output_table_comparison(&computed_stats, &db_entries, filename)?,
    }

    Ok(())
}

fn analyze_fits_directory(
    db: &Database,
    fits_dir: &Path,
    project_filter: Option<String>,
    target_filter: Option<String>,
    format: &str,
    detector: &str,
    sensitivity: &str,
    apply_stretch: bool,
) -> Result<()> {
    let mut fits_files = Vec::new();
    find_fits_files(fits_dir, &mut fits_files)?;

    if fits_files.is_empty() {
        println!("No FITS files found in directory: {}", fits_dir.display());
        return Ok(());
    }

    println!(
        "Analyzing {} FITS files in: {}",
        fits_files.len(),
        fits_dir.display()
    );

    match format.to_lowercase().as_str() {
        "csv" => {
            println!("filename,computed_hfr,computed_stars,computed_mean,computed_median,computed_stddev,db_hfr,db_stars,db_status,project,target,hfr_diff,star_diff");
        }
        "json" => {
            println!("[");
        }
        _ => {}
    }

    let mut results = Vec::new();
    let mut processed_count = 0;
    let mut error_count = 0;

    for (index, fits_path) in fits_files.iter().enumerate() {
        let filename = fits_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        match analyze_fits_file(
            db,
            fits_path,
            filename,
            &project_filter,
            &target_filter,
            detector,
            sensitivity,
            apply_stretch,
        ) {
            Ok(result) => {
                processed_count += 1;

                match format.to_lowercase().as_str() {
                    "json" => {
                        let json_result = serde_json::to_string_pretty(&result)?;
                        print!("{}", json_result);
                        if index < fits_files.len() - 1 {
                            println!(",");
                        } else {
                            println!();
                        }
                    }
                    "csv" => {
                        output_csv_result(&result)?;
                    }
                    _ => {
                        results.push(result);
                    }
                }
            }
            Err(_) => {
                error_count += 1;
                if format == "table" {
                    eprintln!("Error processing: {}", filename);
                }
            }
        }
    }

    match format.to_lowercase().as_str() {
        "json" => {
            println!("]");
        }
        "csv" => {
            // CSV output already printed per-file
        }
        _ => {
            // Print table format summary
            println!(
                "\n{:<30} {:<8} {:<8} {:<8} {:<8} {:<12} {:<12}",
                "Filename", "C_HFR", "C_Stars", "DB_HFR", "DB_Stars", "HFR_Diff", "Status"
            );
            println!("{:-<100}", "");

            for result in results {
                println!(
                    "{:<30} {:<8.3} {:<8} {:<8.3} {:<8} {:<12.3} {:<12}",
                    truncate_string(&result.filename, 30),
                    result.computed_stats.hfr.unwrap_or(0.0),
                    result.computed_stats.star_count.unwrap_or(0),
                    result.database_hfr.unwrap_or(0.0),
                    result.database_stars.unwrap_or(0),
                    result.hfr_difference.unwrap_or(0.0),
                    result.database_status.unwrap_or("Unknown".to_string())
                );
            }

            println!("\nSummary:");
            println!("  Successfully processed: {}", processed_count);
            if error_count > 0 {
                println!("  Errors: {}", error_count);
            }
        }
    }

    Ok(())
}

#[derive(Debug, serde::Serialize)]
struct AnalysisResult {
    filename: String,
    computed_stats: ComputedStats,
    database_hfr: Option<f64>,
    database_stars: Option<i32>,
    database_status: Option<String>,
    database_project: Option<String>,
    database_target: Option<String>,
    hfr_difference: Option<f64>,
    star_difference: Option<i32>,
    found_in_database: bool,
}

fn analyze_fits_file(
    db: &Database,
    fits_path: &Path,
    filename: &str,
    project_filter: &Option<String>,
    target_filter: &Option<String>,
    detector: &str,
    sensitivity: &str,
    apply_stretch: bool,
) -> Result<AnalysisResult> {
    // Load and analyze the FITS image
    let image = FitsImage::from_file(fits_path)?;
    let mut computed_stats = image.calculate_statistics();

    // Perform star detection
    perform_star_detection(
        &image,
        &mut computed_stats,
        detector,
        sensitivity,
        apply_stretch,
    )?;

    // Try to find corresponding database entry
    let db_entries =
        find_database_entry(db, filename, project_filter.clone(), target_filter.clone())?;

    let (
        database_hfr,
        database_stars,
        database_status,
        database_project,
        database_target,
        hfr_diff,
        star_diff,
        found,
    ) = if let Some(entry) = db_entries.first() {
        let db_hfr = entry.hfr;
        let db_stars = entry.detected_stars;
        let hfr_diff = if let (Some(computed), Some(db)) = (computed_stats.hfr, db_hfr) {
            Some(computed - db)
        } else {
            None
        };
        let star_diff = if let (Some(computed), Some(db)) = (computed_stats.star_count, db_stars) {
            Some(computed as i32 - db)
        } else {
            None
        };

        (
            db_hfr,
            db_stars,
            Some(match entry.grading_status {
                0 => "Pending".to_string(),
                1 => "Accepted".to_string(),
                2 => "Rejected".to_string(),
                _ => "Unknown".to_string(),
            }),
            Some(entry.project_name.clone()),
            Some(entry.target_name.clone()),
            hfr_diff,
            star_diff,
            true,
        )
    } else {
        (None, None, None, None, None, None, None, false)
    };

    Ok(AnalysisResult {
        filename: filename.to_string(),
        computed_stats,
        database_hfr,
        database_stars,
        database_status,
        database_project,
        database_target,
        hfr_difference: hfr_diff,
        star_difference: star_diff,
        found_in_database: found,
    })
}

#[derive(Debug)]
struct DatabaseEntry {
    hfr: Option<f64>,
    detected_stars: Option<i32>,
    grading_status: i32,
    project_name: String,
    target_name: String,
}

fn find_database_entry(
    db: &Database,
    filename: &str,
    project_filter: Option<String>,
    target_filter: Option<String>,
) -> Result<Vec<DatabaseEntry>> {
    // Query database for images with matching filename
    let images = db.query_images(
        None,
        project_filter.as_deref(),
        target_filter.as_deref(),
        None,
    )?;

    let mut matches = Vec::new();

    for (image, project_name, target_name) in images {
        // Parse metadata JSON to check filename
        if let Ok(metadata_json) = serde_json::from_str::<serde_json::Value>(&image.metadata) {
            if let Some(db_filename) = metadata_json.get("FileName").and_then(|f| f.as_str()) {
                let db_filename = db_filename
                    .split(&['\\', '/'][..])
                    .next_back()
                    .unwrap_or(db_filename);

                if db_filename == filename {
                    let hfr = metadata_json.get("HFR").and_then(|h| h.as_f64());
                    let detected_stars = metadata_json
                        .get("DetectedStars")
                        .and_then(|s| s.as_i64())
                        .map(|s| s as i32);

                    matches.push(DatabaseEntry {
                        hfr,
                        detected_stars,
                        grading_status: image.grading_status,
                        project_name,
                        target_name,
                    });
                }
            }
        }
    }

    Ok(matches)
}

fn find_fits_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
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

fn output_table_comparison(
    computed: &ComputedStats,
    db_entries: &[DatabaseEntry],
    filename: &str,
) -> Result<()> {
    println!("\nComputed Statistics:");
    println!("  Dimensions: {}x{}", computed.width, computed.height);
    println!("  Mean: {:.3}", computed.mean);
    println!("  Median: {:.3}", computed.median);
    println!("  Std Dev: {:.3}", computed.std_dev);
    println!("  Min: {:.3}", computed.min);
    println!("  Max: {:.3}", computed.max);

    if let Some(hfr) = computed.hfr {
        println!("  HFR: {:.3}", hfr);
    } else {
        println!("  HFR: Not computed (no stars found)");
    }

    if let Some(star_count) = computed.star_count {
        println!("  Stars: {}", star_count);
    } else {
        println!("  Stars: 0");
    }

    if !db_entries.is_empty() {
        println!("\nDatabase Comparison:");
        for (i, entry) in db_entries.iter().enumerate() {
            if db_entries.len() > 1 {
                println!("  Match {}:", i + 1);
            }
            println!("  Project: {}", entry.project_name);
            println!("  Target: {}", entry.target_name);
            println!(
                "  Status: {}",
                match entry.grading_status {
                    0 => "Pending",
                    1 => "Accepted",
                    2 => "Rejected",
                    _ => "Unknown",
                }
            );

            if let Some(db_hfr) = entry.hfr {
                if let Some(computed_hfr) = computed.hfr {
                    println!(
                        "  HFR: {:.3} (computed) vs {:.3} (database) = {:.3} difference",
                        computed_hfr,
                        db_hfr,
                        computed_hfr - db_hfr
                    );
                } else {
                    println!("  HFR: Not computed vs {:.3} (database)", db_hfr);
                }
            } else {
                println!("  HFR: Not in database");
            }

            if let Some(db_stars) = entry.detected_stars {
                if let Some(computed_stars) = computed.star_count {
                    println!(
                        "  Stars: {} (computed) vs {} (database) = {} difference",
                        computed_stars,
                        db_stars,
                        computed_stars as i32 - db_stars
                    );
                } else {
                    println!("  Stars: 0 (computed) vs {} (database)", db_stars);
                }
            } else {
                println!("  Stars: Not in database");
            }
        }
    } else {
        println!("\nDatabase: No matching entry found for {}", filename);
    }

    Ok(())
}

fn output_json_comparison(
    computed: &ComputedStats,
    db_entries: &[DatabaseEntry],
    filename: &str,
) -> Result<()> {
    let result = AnalysisResult {
        filename: filename.to_string(),
        computed_stats: computed.clone(),
        database_hfr: db_entries.first().and_then(|e| e.hfr),
        database_stars: db_entries.first().and_then(|e| e.detected_stars),
        database_status: db_entries.first().map(|e| match e.grading_status {
            0 => "Pending".to_string(),
            1 => "Accepted".to_string(),
            2 => "Rejected".to_string(),
            _ => "Unknown".to_string(),
        }),
        database_project: db_entries.first().map(|e| e.project_name.clone()),
        database_target: db_entries.first().map(|e| e.target_name.clone()),
        hfr_difference: if let (Some(computed_hfr), Some(db_entry)) =
            (computed.hfr, db_entries.first())
        {
            db_entry.hfr.map(|db_hfr| computed_hfr - db_hfr)
        } else {
            None
        },
        star_difference: if let (Some(computed_stars), Some(db_entry)) =
            (computed.star_count, db_entries.first())
        {
            db_entry
                .detected_stars
                .map(|db_stars| computed_stars as i32 - db_stars)
        } else {
            None
        },
        found_in_database: !db_entries.is_empty(),
    };

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn output_csv_comparison(
    computed: &ComputedStats,
    db_entries: &[DatabaseEntry],
    filename: &str,
) -> Result<()> {
    println!("filename,computed_hfr,computed_stars,computed_mean,computed_median,computed_stddev,db_hfr,db_stars,db_status,project,target,hfr_diff,star_diff");

    let result = AnalysisResult {
        filename: filename.to_string(),
        computed_stats: computed.clone(),
        database_hfr: db_entries.first().and_then(|e| e.hfr),
        database_stars: db_entries.first().and_then(|e| e.detected_stars),
        database_status: db_entries.first().map(|e| match e.grading_status {
            0 => "Pending".to_string(),
            1 => "Accepted".to_string(),
            2 => "Rejected".to_string(),
            _ => "Unknown".to_string(),
        }),
        database_project: db_entries.first().map(|e| e.project_name.clone()),
        database_target: db_entries.first().map(|e| e.target_name.clone()),
        hfr_difference: if let (Some(computed_hfr), Some(db_entry)) =
            (computed.hfr, db_entries.first())
        {
            db_entry.hfr.map(|db_hfr| computed_hfr - db_hfr)
        } else {
            None
        },
        star_difference: if let (Some(computed_stars), Some(db_entry)) =
            (computed.star_count, db_entries.first())
        {
            db_entry
                .detected_stars
                .map(|db_stars| computed_stars as i32 - db_stars)
        } else {
            None
        },
        found_in_database: !db_entries.is_empty(),
    };

    output_csv_result(&result)?;
    Ok(())
}

fn output_csv_result(result: &AnalysisResult) -> Result<()> {
    println!(
        "{},{},{},{:.3},{:.3},{:.3},{},{},{},{},{},{},{}",
        escape_csv(&result.filename),
        result
            .computed_stats
            .hfr
            .map(|h| format!("{:.3}", h))
            .unwrap_or_default(),
        result.computed_stats.star_count.unwrap_or(0),
        result.computed_stats.mean,
        result.computed_stats.median,
        result.computed_stats.std_dev,
        result
            .database_hfr
            .map(|h| format!("{:.3}", h))
            .unwrap_or_default(),
        result
            .database_stars
            .map(|s| s.to_string())
            .unwrap_or_default(),
        escape_csv(result.database_status.as_ref().unwrap_or(&String::new())),
        escape_csv(result.database_project.as_ref().unwrap_or(&String::new())),
        escape_csv(result.database_target.as_ref().unwrap_or(&String::new())),
        result
            .hfr_difference
            .map(|d| format!("{:.3}", d))
            .unwrap_or_default(),
        result
            .star_difference
            .map(|d| d.to_string())
            .unwrap_or_default()
    );
    Ok(())
}

fn escape_csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn perform_star_detection(
    image: &FitsImage,
    stats: &mut ComputedStats,
    detector: &str,
    sensitivity: &str,
    apply_stretch: bool,
) -> Result<()> {
    println!("\nStar Detection:");
    println!("  Algorithm: {}", detector);
    println!("  Sensitivity: {}", sensitivity);
    println!("  Apply MTF Stretch: {}", apply_stretch);

    match detector.to_lowercase().as_str() {
        "nina" => {
            // Parse sensitivity
            let star_sensitivity = match sensitivity.to_lowercase().as_str() {
                "high" => StarSensitivity::High,
                "highest" => StarSensitivity::Highest,
                _ => StarSensitivity::Normal,
            };
            println!("  Forcing stretch for NINA");

            let params = StarDetectionParams {
                sensitivity: star_sensitivity,
                noise_reduction: NoiseReduction::None,
                use_roi: false,
            };

            let detection_data ={
                // Apply MTF stretching
                let basic_stats = image.calculate_basic_statistics();
                let stretch_params = StretchParameters::default();
                stretch_image(
                    &image.data,
                    &basic_stats,
                    stretch_params.factor,
                    stretch_params.black_clipping,
                )
            };

            let result = detect_stars_with_original(
                &detection_data,
                &image.data,
                image.width,
                image.height,
                &params,
            );

            println!("  Detected stars: {}", result.star_list.len());
            println!("  Average HFR: {:.3}", result.average_hfr);
            println!("  HFR StdDev: {:.3}", result.hfr_std_dev);

            // Update stats with detection results
            stats.star_count = Some(result.star_list.len());
            stats.hfr = Some(result.average_hfr);

            // Show first few stars if any detected
            if !result.star_list.is_empty() && result.star_list.len() <= 5 {
                println!("\n  Star positions:");
                for (i, star) in result.star_list.iter().enumerate() {
                    println!(
                        "    #{}: ({:.1}, {:.1}) HFR: {:.2}",
                        i + 1,
                        star.position.0,
                        star.position.1,
                        star.hfr
                    );
                }
            } else if result.star_list.len() > 5 {
                println!("\n  First 5 star positions:");
                for (i, star) in result.star_list.iter().take(5).enumerate() {
                    println!(
                        "    #{}: ({:.1}, {:.1}) HFR: {:.2}",
                        i + 1,
                        star.position.0,
                        star.position.1,
                        star.hfr
                    );
                }
            }
        }
        "hocusfocus" => {
            let params = HocusFocusParams::default();

            let result = detect_stars_hocus_focus(&image.data, image.width, image.height, &params);

            println!("  Detected stars: {}", result.stars.len());
            println!("  Average HFR: {:.3}", result.average_hfr);
            println!("  Average FWHM: {:.3}", result.average_fwhm);
            println!("  Noise Sigma: {:.1}", result.noise_sigma);
            println!("  Background: {:.1}", result.background_mean);

            // Update stats with detection results
            stats.star_count = Some(result.stars.len());
            stats.hfr = Some(result.average_hfr);

            // Show first few stars if any detected
            if !result.stars.is_empty() && result.stars.len() <= 5 {
                println!("\n  Star positions:");
                for (i, star) in result.stars.iter().enumerate() {
                    println!(
                        "    #{}: ({:.1}, {:.1}) HFR: {:.2}, SNR: {:.1}",
                        i + 1,
                        star.position.0,
                        star.position.1,
                        star.hfr,
                        star.snr
                    );
                }
            } else if result.stars.len() > 5 {
                println!("\n  First 5 star positions:");
                for (i, star) in result.stars.iter().take(5).enumerate() {
                    println!(
                        "    #{}: ({:.1}, {:.1}) HFR: {:.2}, SNR: {:.1}",
                        i + 1,
                        star.position.0,
                        star.position.1,
                        star.hfr,
                        star.snr
                    );
                }
            }
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown detector: {}. Valid options are: nina, hocusfocus",
                detector
            ));
        }
    }

    Ok(())
}
