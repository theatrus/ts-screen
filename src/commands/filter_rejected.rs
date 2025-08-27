use crate::db::Database;
use crate::grading;
use crate::models::{AcquiredImage, GradingStatus};
use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn filter_rejected_files(
    conn: &Connection,
    base_dir: &str,
    dry_run: bool,
    project_filter: Option<String>,
    target_filter: Option<String>,
    stat_config: Option<grading::StatisticalGradingConfig>,
) -> Result<()> {
    let db = Database::new(conn);

    // If statistical analysis is enabled, we need all images to analyze
    let perform_statistical = stat_config.is_some();

    // Get images - if statistical analysis enabled, get all; otherwise just rejected
    let all_images = if perform_statistical {
        db.query_images(
            None, // Get all statuses for statistical analysis
            project_filter.as_deref(),
            target_filter.as_deref(),
            None,
        )?
    } else {
        db.query_images(
            Some(GradingStatus::Rejected),
            project_filter.as_deref(),
            target_filter.as_deref(),
            None,
        )?
    };

    // Perform statistical analysis if enabled
    let mut statistical_rejections = HashMap::new();
    if let Some(config) = stat_config {
        println!("Performing statistical analysis...");

        // Convert to format expected by grader
        let mut image_stats = Vec::new();
        for (image, _project_name, target_name) in &all_images {
            match grading::parse_image_metadata(
                image.id,
                image.target_id,
                target_name,
                &image.metadata,
                &image.filter_name,
                image.grading_status,
            ) {
                Ok(stats) => image_stats.push(stats),
                Err(e) => println!(
                    "  Warning: Failed to parse metadata for image {}: {}",
                    image.id, e
                ),
            }
        }

        // Run statistical analysis
        let grader = grading::StatisticalGrader::new(config);
        match grader.analyze_images(image_stats) {
            Ok(rejections) => {
                println!("  Found {} statistical rejections", rejections.len());
                for rejection in rejections {
                    println!(
                        "    Image {}: {} - {}",
                        rejection.image_id, rejection.reason, rejection.details
                    );
                    statistical_rejections.insert(rejection.image_id, rejection);
                }
            }
            Err(e) => println!("  Warning: Statistical analysis failed: {}", e),
        }
        println!();
    }

    let mut moved_count = 0;
    let mut not_found_count = 0;
    let mut error_count = 0;

    println!(
        "{}Filtering files...",
        if dry_run { "[DRY RUN] " } else { "" }
    );
    println!();

    for (image, _project_name, _target_name) in all_images {
        // Check if this image should be moved
        let should_move = if perform_statistical {
            // Move if rejected in database OR statistically rejected
            image.grading_status == 2 || statistical_rejections.contains_key(&image.id)
        } else {
            // Move only if rejected in database
            image.grading_status == 2
        };

        if !should_move {
            continue;
        }

        // Process the file movement
        match process_file_movement(&image, base_dir, dry_run, &statistical_rejections) {
            Ok(true) => moved_count += 1,
            Ok(false) => not_found_count += 1,
            Err(e) => {
                println!("  ERROR: {}", e);
                error_count += 1;
            }
        }
    }

    println!("\nSummary:");
    println!("  Files moved: {}", moved_count);
    println!("  Files not found: {}", not_found_count);
    if error_count > 0 {
        println!("  Errors: {}", error_count);
    }

    if dry_run {
        println!("\nThis was a dry run. Use without --dry-run to actually move files.");
    }

    Ok(())
}

fn process_file_movement(
    image: &AcquiredImage,
    base_dir: &str,
    dry_run: bool,
    statistical_rejections: &HashMap<i32, grading::StatisticalRejection>,
) -> Result<bool> {
    let metadata = serde_json::from_str::<serde_json::Value>(&image.metadata)?;

    let filename = metadata["FileName"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No filename in metadata for image {}", image.id))?;

    let acquired_date = image
        .acquired_date
        .and_then(|d| chrono::DateTime::from_timestamp(d, 0))
        .ok_or_else(|| anyhow::anyhow!("Invalid date for image {}", image.id))?;

    let date_str = acquired_date.format("%Y-%m-%d").to_string();

    // Extract just the filename from the full path
    let file_only = PathBuf::from(filename)
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid filename format"))?
        .to_string();

    // Try to find the file in different possible locations
    let possible_paths = get_possible_paths(
        base_dir,
        &date_str,
        metadata["TargetName"].as_str().unwrap_or("Unknown"),
        &file_only,
    );

    let mut source_path = None;
    for path in &possible_paths {
        if path.exists() {
            source_path = Some(path.clone());
            break;
        }
    }

    let source_path = match source_path {
        Some(path) => path,
        None => {
            let rejection_reason =
                if let Some(stat_rejection) = statistical_rejections.get(&image.id) {
                    format!("{} - {}", stat_rejection.reason, stat_rejection.details)
                } else {
                    image
                        .reject_reason
                        .clone()
                        .unwrap_or_else(|| "No reason".to_string())
                };

            println!(
                "  {:6} NOT FOUND: {} ({})",
                image.id, file_only, rejection_reason
            );
            return Ok(false);
        }
    };

    // Create the reject path by replacing LIGHT with LIGHT_REJECT
    let reject_path = get_reject_path(&source_path)?;

    let rejection_reason = if let Some(stat_rejection) = statistical_rejections.get(&image.id) {
        format!("{} - {}", stat_rejection.reason, stat_rejection.details)
    } else {
        image
            .reject_reason
            .clone()
            .unwrap_or_else(|| "No reason".to_string())
    };

    println!(
        "  {:6} {} -> {}",
        image.id,
        source_path.display(),
        reject_path.display()
    );
    println!("         Reason: {}", rejection_reason);

    if !dry_run {
        // Create the reject directory if it doesn't exist
        if let Some(parent) = reject_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Move the file
        fs::rename(&source_path, &reject_path)?;
    }

    Ok(true)
}

fn get_possible_paths(
    base_dir: &str,
    date_str: &str,
    target_name: &str,
    filename: &str,
) -> Vec<PathBuf> {
    let base = PathBuf::from(base_dir);

    vec![
        // date/target/date/LIGHT/file.fits
        base.join(date_str)
            .join(target_name)
            .join(date_str)
            .join("LIGHT")
            .join(filename),
        // target/date/LIGHT/file.fits
        base.join(target_name)
            .join(date_str)
            .join("LIGHT")
            .join(filename),
        // date/target/date/LIGHT/rejected/file.fits
        base.join(date_str)
            .join(target_name)
            .join(date_str)
            .join("LIGHT")
            .join("rejected")
            .join(filename),
        // target/date/LIGHT/rejected/file.fits
        base.join(target_name)
            .join(date_str)
            .join("LIGHT")
            .join("rejected")
            .join(filename),
    ]
}

fn get_reject_path(source_path: &Path) -> Result<PathBuf> {
    let path_str = source_path.to_string_lossy();

    // If the file is already in a rejected subdirectory, move it up to LIGHT_REJECT
    if path_str.contains("/LIGHT/rejected/") || path_str.contains("\\LIGHT\\rejected\\") {
        Ok(PathBuf::from(
            path_str
                .replace("/LIGHT/rejected/", "/LIGHT_REJECT/")
                .replace("\\LIGHT\\rejected\\", "\\LIGHT_REJECT\\"),
        ))
    } else {
        // Replace LIGHT with LIGHT_REJECT
        Ok(PathBuf::from(
            path_str
                .replace("/LIGHT/", "/LIGHT_REJECT/")
                .replace("\\LIGHT\\", "\\LIGHT_REJECT\\"),
        ))
    }
}
