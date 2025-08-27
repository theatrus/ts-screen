use crate::grading;
use crate::models::AcquiredImage;
use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub fn filter_rejected_files(
    conn: &Connection,
    base_dir: &str,
    dry_run: bool,
    project_filter: Option<String>,
    target_filter: Option<String>,
    stat_config: Option<grading::StatisticalGradingConfig>,
) -> Result<()> {
    // If statistical analysis is enabled, we need all images to analyze
    let perform_statistical = stat_config.is_some();

    // Query for images - if statistical analysis enabled, get all; otherwise just rejected
    let mut query = String::from(
        "SELECT ai.Id, ai.projectId, ai.targetId, ai.acquireddate, ai.filtername, 
                ai.gradingStatus, ai.metadata, ai.rejectreason, ai.profileId,
                p.name as project_name, t.name as target_name
         FROM acquiredimage ai
         JOIN project p ON ai.projectId = p.Id
         JOIN target t ON ai.targetId = t.Id",
    );

    if !perform_statistical {
        query.push_str(" WHERE ai.gradingStatus = 2"); // 2 = Rejected
    } else {
        query.push_str(" WHERE 1=1"); // Get all images for statistical analysis
    }

    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(project) = &project_filter {
        query.push_str(" AND p.name LIKE ?");
        params.push(Box::new(format!("%{}%", project)));
    }

    if let Some(target) = &target_filter {
        query.push_str(" AND t.name LIKE ?");
        params.push(Box::new(format!("%{}%", target)));
    }

    query.push_str(" ORDER BY ai.acquireddate DESC");

    let mut stmt = conn.prepare(&query)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let image_iter = stmt.query_map(param_refs.as_slice(), |row| {
        Ok((
            AcquiredImage {
                id: row.get(0)?,
                project_id: row.get(1)?,
                target_id: row.get(2)?,
                acquired_date: row.get(3)?,
                filter_name: row.get(4)?,
                grading_status: row.get(5)?,
                metadata: row.get(6)?,
                reject_reason: row.get(7)?,
                profile_id: row.get(8)?,
            },
            row.get::<_, String>(9)?,  // project_name
            row.get::<_, String>(10)?, // target_name
        ))
    })?;

    // Collect all images first
    let mut all_images = Vec::new();
    for image_result in image_iter {
        all_images.push(image_result?);
    }

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

    println!();
    println!("Summary:");
    println!("  Files to move: {}", moved_count);
    println!("  Files not found: {}", not_found_count);
    if !dry_run && error_count > 0 {
        println!("  Errors: {}", error_count);
    }

    if dry_run && moved_count > 0 {
        println!();
        println!("This was a dry run. Use without --dry-run to actually move the files.");
    }

    Ok(())
}

fn process_file_movement(
    image: &AcquiredImage,
    base_dir: &str,
    dry_run: bool,
    statistical_rejections: &HashMap<i32, grading::StatisticalRejection>,
) -> Result<bool> {
    // Extract the full file path from metadata
    let metadata: serde_json::Value = serde_json::from_str(&image.metadata)?;
    let original_path = metadata
        .get("FileName")
        .and_then(|f| f.as_str())
        .ok_or_else(|| anyhow::anyhow!("No filename in metadata for image {}", image.id))?;

    // Parse the original path to understand the structure
    let path_parts: Vec<&str> = original_path.split(&['\\', '/'][..]).collect();

    // Find the LIGHT directory index
    let _light_idx = path_parts
        .iter()
        .rposition(|&p| p == "LIGHT")
        .ok_or_else(|| anyhow::anyhow!("LIGHT directory not found in path: {}", original_path))?;

    // Extract the filename
    let filename = path_parts
        .last()
        .ok_or_else(|| anyhow::anyhow!("No filename in path: {}", original_path))?;

    // Build the paths
    let (source_path, dest_path, actual_source_path) =
        build_file_paths(base_dir, &path_parts, filename)?;

    if actual_source_path.is_none() {
        println!("  NOT FOUND: {}", source_path.display());
        return Ok(false);
    }

    let actual_source_path = actual_source_path.unwrap();

    // Create destination directory if needed
    let dest_dir = dest_path.parent().unwrap();

    // Determine the rejection reason
    let rejection_reason = if let Some(stat_rejection) = statistical_rejections.get(&image.id) {
        format!("{}: {}", stat_rejection.reason, stat_rejection.details)
    } else {
        image
            .reject_reason
            .as_deref()
            .unwrap_or("No reason given")
            .to_string()
    };

    if dry_run {
        println!(
            "  WOULD MOVE: {} -> {}",
            actual_source_path.display(),
            dest_path.display()
        );
        println!("    Reason: {}", rejection_reason);
    } else {
        // Create destination directory
        fs::create_dir_all(dest_dir)?;

        // Move the file
        match fs::rename(&actual_source_path, &dest_path) {
            Ok(_) => {
                println!(
                    "  MOVED: {} -> {}",
                    actual_source_path.display(),
                    dest_path.display()
                );
                println!("    Reason: {}", rejection_reason);
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to move {}: {}",
                    actual_source_path.display(),
                    e
                ));
            }
        }
    }

    Ok(true)
}

fn build_file_paths(
    base_dir: &str,
    path_parts: &[&str],
    filename: &str,
) -> Result<(PathBuf, PathBuf, Option<PathBuf>)> {
    let mut source_path = PathBuf::new();
    let mut dest_path = PathBuf::new();

    // Try standard structure: date/target_name/date/LIGHT/filename
    if path_parts.len() >= 5 {
        let relative_parts: Vec<&str> = path_parts[path_parts.len() - 5..].to_vec();
        source_path = PathBuf::from(base_dir)
            .join(relative_parts[0]) // First date
            .join(relative_parts[1]) // Target name
            .join(relative_parts[2]) // Second date
            .join("LIGHT")
            .join(filename);

        dest_path = PathBuf::from(base_dir)
            .join(relative_parts[0]) // First date
            .join(relative_parts[1]) // Target name
            .join(relative_parts[2]) // Second date
            .join("LIGHT_REJECT")
            .join(filename);
    }

    // Check if source exists
    let mut actual_source_path = None;
    if source_path.exists() {
        actual_source_path = Some(source_path.clone());
    } else {
        // Try alternate structure: target_name/date/LIGHT/filename
        if let Some((target_name, date)) = extract_target_and_date(path_parts, filename) {
            let alt_source_path = PathBuf::from(base_dir)
                .join(target_name)
                .join(date)
                .join("LIGHT")
                .join(filename);

            if alt_source_path.exists() {
                source_path = alt_source_path;
                dest_path = PathBuf::from(base_dir)
                    .join(target_name)
                    .join(date)
                    .join("LIGHT_REJECT")
                    .join(filename);
                actual_source_path = Some(source_path.clone());
            } else {
                // Check rejected subdirectory
                let rejected_path = PathBuf::from(base_dir)
                    .join(target_name)
                    .join(date)
                    .join("LIGHT")
                    .join("rejected")
                    .join(filename);

                if rejected_path.exists() {
                    actual_source_path = Some(rejected_path);
                }
            }
        }
    }

    // Also check standard rejected subdirectory
    if actual_source_path.is_none() && source_path.parent().is_some() {
        let rejected_path = source_path
            .parent()
            .unwrap()
            .join("rejected")
            .join(filename);
        if rejected_path.exists() {
            actual_source_path = Some(rejected_path);
        }
    }

    Ok((source_path, dest_path, actual_source_path))
}

fn extract_target_and_date<'a>(
    path_parts: &[&'a str],
    filename: &'a str,
) -> Option<(&'a str, &'a str)> {
    let mut target_name = None;
    let mut date = None;

    // Look for date pattern in path
    for (i, part) in path_parts.iter().enumerate() {
        if part.len() == 10 && part.chars().nth(4) == Some('-') && part.chars().nth(7) == Some('-')
        {
            date = Some(*part);
            // The target name is likely before the date
            if i > 0 {
                target_name = Some(path_parts[i - 1]);
            }
        }
    }

    // Also check filename for date if not found
    if date.is_none() && filename.len() > 10 {
        let potential_date = &filename[0..10];
        if potential_date.chars().nth(4) == Some('-') && potential_date.chars().nth(7) == Some('-')
        {
            date = Some(potential_date);
            // Find target name from path
            if target_name.is_none() {
                for part in path_parts.iter().rev() {
                    if *part != "LIGHT"
                        && *part != "rejected"
                        && *part != filename
                        && !(part.len() == 10 && part.chars().nth(4) == Some('-'))
                    {
                        target_name = Some(*part);
                        break;
                    }
                }
            }
        }
    }

    if let (Some(target), Some(date_str)) = (target_name, date) {
        Some((target, date_str))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_target_and_date_standard() {
        let path_parts = vec!["home", "user", "Bubble Nebula", "2023-08-27", "LIGHT"];
        let filename = "image.fits";

        let result = extract_target_and_date(&path_parts, filename);
        assert_eq!(result, Some(("Bubble Nebula", "2023-08-27")));
    }

    #[test]
    fn test_extract_target_and_date_from_filename() {
        let path_parts = vec!["home", "user", "NGC 7000", "LIGHT"];
        let filename = "2023-08-27_NGC7000_Ha_300s.fits";

        let result = extract_target_and_date(&path_parts, filename);
        assert_eq!(result, Some(("NGC 7000", "2023-08-27")));
    }

    #[test]
    fn test_extract_target_and_date_no_date() {
        let path_parts = vec!["home", "user", "target", "LIGHT"];
        let filename = "image.fits";

        let result = extract_target_and_date(&path_parts, filename);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_target_and_date_multiple_dates() {
        let path_parts = vec!["2023-08-26", "Target Name", "2023-08-27", "LIGHT"];
        let filename = "image.fits";

        let result = extract_target_and_date(&path_parts, filename);
        assert_eq!(result, Some(("Target Name", "2023-08-27")));
    }

    #[test]
    fn test_extract_target_and_date_with_rejected() {
        let path_parts = vec!["Target", "2023-08-27", "LIGHT", "rejected"];
        let filename = "image.fits";

        let result = extract_target_and_date(&path_parts, filename);
        assert_eq!(result, Some(("Target", "2023-08-27")));
    }
}
