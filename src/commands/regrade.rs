use anyhow::Result;
use rusqlite::{Connection, params};
use crate::models::AcquiredImage;
use crate::grading;

pub fn regrade_images(
    conn: &Connection,
    dry_run: bool,
    target_filter: Option<String>,
    project_filter: Option<String>,
    days: u32,
    reset_mode: &str,
    stat_config: Option<grading::StatisticalGradingConfig>,
) -> Result<()> {
    // Validate reset mode
    match reset_mode {
        "none" | "automatic" | "all" => {},
        _ => return Err(anyhow::anyhow!("Invalid reset mode: {}. Use 'none', 'automatic', or 'all'", reset_mode)),
    }
    
    println!("{}Regrading images...", if dry_run { "[DRY RUN] " } else { "" });
    
    // Calculate date cutoff
    let now = chrono::Utc::now();
    let cutoff_date = now - chrono::Duration::days(days as i64);
    let cutoff_timestamp = cutoff_date.timestamp();
    
    println!("  Date range: {} to now", cutoff_date.format("%Y-%m-%d"));
    
    // First, handle reset if requested
    if reset_mode != "none" {
        handle_reset(conn, dry_run, reset_mode, cutoff_timestamp, &project_filter, &target_filter)?;
    }
    
    // Now perform statistical grading if enabled
    if let Some(config) = stat_config {
        perform_statistical_grading(
            conn, 
            dry_run, 
            cutoff_timestamp, 
            &project_filter, 
            &target_filter, 
            config
        )?;
    }
    
    println!("\nRegrading complete.");
    
    if dry_run {
        println!("\nThis was a dry run. Use without --dry-run to actually update the database.");
    }
    
    Ok(())
}

fn handle_reset(
    conn: &Connection,
    dry_run: bool,
    reset_mode: &str,
    cutoff_timestamp: i64,
    project_filter: &Option<String>,
    target_filter: &Option<String>,
) -> Result<()> {
    println!("  Reset mode: {}", reset_mode);
    
    let mut reset_query = String::from(
        "UPDATE acquiredimage 
         SET gradingStatus = 0, rejectreason = NULL 
         WHERE acquireddate >= ?"
    );
    
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(cutoff_timestamp)];
    
    // Add filters
    if let Some(project) = project_filter {
        reset_query.push_str(" AND projectId IN (SELECT Id FROM project WHERE name LIKE ?)");
        params.push(Box::new(format!("%{}%", project)));
    }
    
    if let Some(target) = target_filter {
        reset_query.push_str(" AND targetId IN (SELECT Id FROM target WHERE name LIKE ?)");
        params.push(Box::new(format!("%{}%", target)));
    }
    
    // For automatic mode, only reset non-manual rejections
    if reset_mode == "automatic" {
        // Assume manual rejections have specific reject reasons
        // Also include previously auto-rejected items
        reset_query.push_str(" AND (gradingStatus = 2 AND (rejectreason IS NULL OR rejectreason LIKE '%[Auto]%' OR (rejectreason NOT LIKE '%Manual%' AND rejectreason NOT LIKE '%manual%')))");
    }
    
    if dry_run {
        // Count how many would be reset
        let count_query = reset_query.replace(
            "UPDATE acquiredimage SET gradingStatus = 0, rejectreason = NULL", 
            "SELECT COUNT(*) FROM acquiredimage"
        );
        let mut stmt = conn.prepare(&count_query)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let count: i32 = stmt.query_row(param_refs.as_slice(), |row| row.get(0))
            .unwrap_or(0);
        println!("  Would reset {} images to pending status", count);
    } else {
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let affected = conn.execute(&reset_query, param_refs.as_slice())?;
        println!("  Reset {} images to pending status", affected);
    }
    
    Ok(())
}

fn perform_statistical_grading(
    conn: &Connection,
    dry_run: bool,
    cutoff_timestamp: i64,
    project_filter: &Option<String>,
    target_filter: &Option<String>,
    config: grading::StatisticalGradingConfig,
) -> Result<()> {
    println!("\nPerforming statistical analysis...");
    
    // Query for images in date range
    let mut query = String::from(
        "SELECT ai.Id, ai.projectId, ai.targetId, ai.acquireddate, ai.filtername, 
                ai.gradingStatus, ai.metadata, ai.rejectreason, ai.profileId,
                p.name as project_name, t.name as target_name
         FROM acquiredimage ai
         JOIN project p ON ai.projectId = p.Id
         JOIN target t ON ai.targetId = t.Id
         WHERE ai.acquireddate >= ?"
    );
    
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(cutoff_timestamp)];
    
    if let Some(project) = project_filter {
        query.push_str(" AND p.name LIKE ?");
        params.push(Box::new(format!("%{}%", project)));
    }
    
    if let Some(target) = target_filter {
        query.push_str(" AND t.name LIKE ?");
        params.push(Box::new(format!("%{}%", target)));
    }
    
    query.push_str(" ORDER BY ai.acquireddate DESC");
    
    let mut stmt = conn.prepare(&query)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    
    let image_iter = stmt.query_map(
        param_refs.as_slice(),
        |row| {
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
                row.get::<_, String>(9)?, // project_name
                row.get::<_, String>(10)?, // target_name
            ))
        }
    )?;
    
    // Collect all images
    let mut all_images = Vec::new();
    for image_result in image_iter {
        all_images.push(image_result?);
    }
    
    println!("  Analyzing {} images", all_images.len());
    
    // Convert to format expected by grader
    let mut image_stats = Vec::new();
    for (image, _project_name, target_name) in &all_images {
        match grading::parse_image_metadata(
            image.id,
            image.target_id,
            target_name,
            &image.metadata, 
            &image.filter_name, 
            image.grading_status
        ) {
            Ok(stats) => image_stats.push(stats),
            Err(e) => println!("  Warning: Failed to parse metadata for image {}: {}", image.id, e),
        }
    }
    
    // Run statistical analysis
    let grader = grading::StatisticalGrader::new(config);
    match grader.analyze_images(image_stats) {
        Ok(rejections) => {
            println!("  Found {} statistical rejections", rejections.len());
            
            if dry_run {
                // Show what would be updated
                for rejection in &rejections {
                    println!("    Would reject image {}: {} - {}", 
                        rejection.image_id, rejection.reason, rejection.details);
                }
            } else {
                // Update database with rejections
                let update_query = "UPDATE acquiredimage SET gradingStatus = 2, rejectreason = ? WHERE Id = ?";
                let mut updated_count = 0;
                
                for rejection in rejections {
                    let reason = format!("[Auto] {} - {}", rejection.reason, rejection.details);
                    match conn.execute(update_query, params![reason, rejection.image_id]) {
                        Ok(_) => updated_count += 1,
                        Err(e) => println!("    Error updating image {}: {}", rejection.image_id, e),
                    }
                }
                
                println!("  Updated {} images with rejection status", updated_count);
            }
        },
        Err(e) => println!("  Warning: Statistical analysis failed: {}", e),
    }
    
    Ok(())
}