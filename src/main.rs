use clap::{Parser, Subcommand};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use anyhow::Context;
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Serialize, Deserialize)]
struct Project {
    id: i32,
    profile_id: String,
    name: String,
    description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Target {
    id: i32,
    name: String,
    active: bool,
    ra: Option<f64>,
    dec: Option<f64>,
    project_id: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct AcquiredImage {
    id: i32,
    project_id: i32,
    target_id: i32,
    acquired_date: Option<i64>,
    filter_name: String,
    grading_status: i32,
    metadata: String,
    reject_reason: Option<String>,
    profile_id: Option<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
enum GradingStatus {
    Pending = 0,
    Accepted = 1,
    Rejected = 2,
}

impl GradingStatus {
    fn from_i32(value: i32) -> &'static str {
        match value {
            0 => "Pending",
            1 => "Accepted",
            2 => "Rejected",
            _ => "Unknown",
        }
    }
}

#[derive(Parser)]
#[command(name = "ts-screen")]
#[command(about = "A tool to analyze telescope scheduler database", long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "schedulerdb.sqlite")]
    database: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Dump grading results for all images
    DumpGrading {
        /// Show only specific grading status (pending, accepted, rejected)
        #[arg(short, long)]
        status: Option<String>,
        
        /// Filter by project name
        #[arg(short, long)]
        project: Option<String>,
        
        /// Filter by target name
        #[arg(short, long)]
        target: Option<String>,
        
        /// Output format (json, csv, table)
        #[arg(short, long, default_value = "table")]
        format: String,
    },
    
    /// List all projects
    ListProjects,
    
    /// List targets for a specific project
    ListTargets {
        /// Project ID or name
        project: String,
    },
    
    /// Filter rejected files and move them to LIGHT_REJECT folders
    FilterRejected {
        /// Database file to use
        database: String,
        
        /// Base directory containing the image files
        base_dir: String,
        
        /// Perform a dry run (show what would be moved without actually moving)
        #[arg(long)]
        dry_run: bool,
        
        /// Filter by project name
        #[arg(short, long)]
        project: Option<String>,
        
        /// Filter by target name
        #[arg(short, long)]
        target: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::DumpGrading { status, project, target, format } => {
            let conn = Connection::open(&cli.database)
                .with_context(|| format!("Failed to open database: {}", cli.database))?;
            dump_grading_results(&conn, status, project, target, &format)?;
        },
        Commands::ListProjects => {
            let conn = Connection::open(&cli.database)
                .with_context(|| format!("Failed to open database: {}", cli.database))?;
            list_projects(&conn)?;
        },
        Commands::ListTargets { project } => {
            let conn = Connection::open(&cli.database)
                .with_context(|| format!("Failed to open database: {}", cli.database))?;
            list_targets(&conn, &project)?;
        },
        Commands::FilterRejected { database, base_dir, dry_run, project, target } => {
            let conn = Connection::open(&database)
                .with_context(|| format!("Failed to open database: {}", database))?;
            filter_rejected_files(&conn, &base_dir, dry_run, project, target)?;
        },
    }
    
    Ok(())
}

fn dump_grading_results(
    conn: &Connection,
    status_filter: Option<String>,
    project_filter: Option<String>,
    target_filter: Option<String>,
    format: &str,
) -> anyhow::Result<()> {
    let mut query = String::from(
        "SELECT ai.Id, ai.projectId, ai.targetId, ai.acquireddate, ai.filtername, 
                ai.gradingStatus, ai.metadata, ai.rejectreason, ai.profileId,
                p.name as project_name, t.name as target_name
         FROM acquiredimage ai
         JOIN project p ON ai.projectId = p.Id
         JOIN target t ON ai.targetId = t.Id
         WHERE 1=1"
    );
    
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    
    if let Some(status) = &status_filter {
        let status_value = match status.to_lowercase().as_str() {
            "pending" => 0,
            "accepted" => 1,
            "rejected" => 2,
            _ => return Err(anyhow::anyhow!("Invalid status: {}. Use pending, accepted, or rejected", status)),
        };
        query.push_str(" AND ai.gradingStatus = ?");
        params.push(Box::new(status_value));
    }
    
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
    
    let mut results = Vec::new();
    for image in image_iter {
        results.push(image?);
    }
    
    match format {
        "json" => output_json(&results)?,
        "csv" => output_csv(&results)?,
        "table" | _ => output_table(&results)?,
    }
    
    Ok(())
}

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

fn output_table(results: &[(AcquiredImage, String, String)]) -> anyhow::Result<()> {
    println!("{:<10} {:<50} {:<20} {:<20} {:<15} {:<10} {:<16} {:<20}",
             "ID", "Filename", "Project", "Target", "Filter", "Status", "Date", "Reject Reason");
    println!("{:-<180}", "");
    
    for (image, project_name, target_name) in results {
        let date_str = image.acquired_date
            .and_then(|d| chrono::DateTime::from_timestamp(d, 0))
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "No date".to_string());
        
        let filename = extract_filename(&image.metadata)
            .unwrap_or_else(|| "Unknown".to_string());
        
        println!("{:<10} {:<50} {:<20} {:<20} {:<15} {:<10} {:<16} {:<20}",
                 image.id,
                 truncate_string(&filename, 50),
                 truncate_string(project_name, 20),
                 truncate_string(target_name, 20),
                 truncate_string(&image.filter_name, 15),
                 GradingStatus::from_i32(image.grading_status),
                 date_str,
                 image.reject_reason.as_deref().unwrap_or(""));
    }
    
    println!("\nTotal: {} images", results.len());
    Ok(())
}

fn output_json(results: &[(AcquiredImage, String, String)]) -> anyhow::Result<()> {
    let json_results: Vec<serde_json::Value> = results.iter().map(|(image, project, target)| {
        let filename = extract_filename(&image.metadata)
            .unwrap_or_else(|| "Unknown".to_string());
        
        serde_json::json!({
            "id": image.id,
            "filename": filename,
            "project_id": image.project_id,
            "project_name": project,
            "target_id": image.target_id,
            "target_name": target,
            "filter_name": image.filter_name,
            "grading_status": GradingStatus::from_i32(image.grading_status),
            "grading_status_code": image.grading_status,
            "acquired_date": image.acquired_date,
            "reject_reason": image.reject_reason,
            "metadata": image.metadata,
        })
    }).collect();
    
    println!("{}", serde_json::to_string_pretty(&json_results)?);
    Ok(())
}

fn output_csv(results: &[(AcquiredImage, String, String)]) -> anyhow::Result<()> {
    println!("id,filename,project_name,target_name,filter_name,grading_status,acquired_date,reject_reason");
    
    for (image, project_name, target_name) in results {
        let date_str = image.acquired_date
            .map(|d| d.to_string())
            .unwrap_or_else(|| "".to_string());
        
        let filename = extract_filename(&image.metadata)
            .unwrap_or_else(|| "Unknown".to_string());
        
        println!("{},{},{},{},{},{},{},{}",
                 image.id,
                 filename,
                 project_name,
                 target_name,
                 image.filter_name,
                 GradingStatus::from_i32(image.grading_status),
                 date_str,
                 image.reject_reason.as_deref().unwrap_or(""));
    }
    Ok(())
}

fn list_projects(conn: &Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT Id, profileId, name, description 
         FROM project 
         ORDER BY name"
    )?;
    
    let project_iter = stmt.query_map([], |row| {
        Ok(Project {
            id: row.get(0)?,
            profile_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
        })
    })?;
    
    println!("{:<10} {:<30} {:<20} {:<40}",
             "ID", "Name", "Profile ID", "Description");
    println!("{:-<100}", "");
    
    for project in project_iter {
        let project = project?;
        println!("{:<10} {:<30} {:<20} {:<40}",
                 project.id,
                 truncate_string(&project.name, 30),
                 truncate_string(&project.profile_id, 20),
                 truncate_string(&project.description.unwrap_or_else(|| "".to_string()), 40));
    }
    
    Ok(())
}

fn list_targets(conn: &Connection, project_identifier: &str) -> anyhow::Result<()> {
    let project_id: i32 = if let Ok(id) = project_identifier.parse::<i32>() {
        id
    } else {
        // Try to find by name
        let mut stmt = conn.prepare("SELECT Id FROM project WHERE name = ?")?;
        stmt.query_row([project_identifier], |row| row.get(0))
            .with_context(|| format!("Project '{}' not found", project_identifier))?
    };
    
    let mut stmt = conn.prepare(
        "SELECT t.Id, t.name, t.active, t.ra, t.dec,
                COUNT(ai.Id) as image_count,
                SUM(CASE WHEN ai.gradingStatus = 1 THEN 1 ELSE 0 END) as accepted_count,
                SUM(CASE WHEN ai.gradingStatus = 2 THEN 1 ELSE 0 END) as rejected_count
         FROM target t
         LEFT JOIN acquiredimage ai ON t.Id = ai.targetId
         WHERE t.projectid = ?
         GROUP BY t.Id, t.name, t.active, t.ra, t.dec
         ORDER BY t.name"
    )?;
    
    let target_iter = stmt.query_map([project_id], |row| {
        Ok((
            Target {
                id: row.get(0)?,
                name: row.get(1)?,
                active: row.get(2)?,
                ra: row.get(3)?,
                dec: row.get(4)?,
                project_id,
            },
            row.get::<_, i32>(5)?, // image_count
            row.get::<_, i32>(6)?, // accepted_count
            row.get::<_, i32>(7)?, // rejected_count
        ))
    })?;
    
    println!("{:<10} {:<30} {:<10} {:<15} {:<15} {:<10} {:<10} {:<10}",
             "ID", "Name", "Active", "RA", "Dec", "Images", "Accepted", "Rejected");
    println!("{:-<120}", "");
    
    for target in target_iter {
        let (target, image_count, accepted_count, rejected_count) = target?;
        println!("{:<10} {:<30} {:<10} {:<15.6} {:<15.6} {:<10} {:<10} {:<10}",
                 target.id,
                 truncate_string(&target.name, 30),
                 if target.active { "Yes" } else { "No" },
                 target.ra.unwrap_or(0.0),
                 target.dec.unwrap_or(0.0),
                 image_count,
                 accepted_count,
                 rejected_count);
    }
    
    Ok(())
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len-3])
    }
}

fn filter_rejected_files(
    conn: &Connection,
    base_dir: &str,
    dry_run: bool,
    project_filter: Option<String>,
    target_filter: Option<String>,
) -> anyhow::Result<()> {
    // Query for rejected files
    let mut query = String::from(
        "SELECT ai.Id, ai.projectId, ai.targetId, ai.acquireddate, ai.filtername, 
                ai.gradingStatus, ai.metadata, ai.rejectreason, ai.profileId,
                p.name as project_name, t.name as target_name
         FROM acquiredimage ai
         JOIN project p ON ai.projectId = p.Id
         JOIN target t ON ai.targetId = t.Id
         WHERE ai.gradingStatus = 2"  // 2 = Rejected
    );
    
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
    
    let mut moved_count = 0;
    let mut not_found_count = 0;
    let mut error_count = 0;
    
    println!("{}Filtering rejected files from database...", if dry_run { "[DRY RUN] " } else { "" });
    println!();
    
    for image_result in image_iter {
        let (image, _project_name, _target_name) = image_result?;
        
        // Extract the full file path from metadata
        let metadata: serde_json::Value = serde_json::from_str(&image.metadata)?;
        let original_path = metadata.get("FileName")
            .and_then(|f| f.as_str())
            .ok_or_else(|| anyhow::anyhow!("No filename in metadata for image {}", image.id))?;
        
        // Parse the original path to understand the structure
        let path_parts: Vec<&str> = original_path.split(&['\\', '/'][..]).collect();
        
        // Find the LIGHT directory index
        let _light_idx = path_parts.iter().rposition(|&p| p == "LIGHT")
            .ok_or_else(|| anyhow::anyhow!("LIGHT directory not found in path: {}", original_path))?;
        
        // Extract the date folders and filename
        // Typical structure: .../2025-08-16/Target Name/2025-08-16/LIGHT/filename.fits
        let filename = path_parts.last()
            .ok_or_else(|| anyhow::anyhow!("No filename in path: {}", original_path))?;
        
        // Build the relative path from base_dir
        // First try standard structure: date/target_name/date/LIGHT/filename
        let mut source_path = PathBuf::new();
        let mut dest_path = PathBuf::new();
        let mut found_structure = false;
        
        if path_parts.len() >= 5 {
            let relative_parts: Vec<&str> = path_parts[path_parts.len() - 5..].to_vec();
            source_path = PathBuf::from(base_dir)
                .join(&relative_parts[0])  // First date
                .join(&relative_parts[1])  // Target name
                .join(&relative_parts[2])  // Second date
                .join("LIGHT")
                .join(filename);
            
            dest_path = PathBuf::from(base_dir)
                .join(&relative_parts[0])  // First date
                .join(&relative_parts[1])  // Target name
                .join(&relative_parts[2])  // Second date
                .join("LIGHT_REJECT")
                .join(filename);
            
            found_structure = true;
        }
        
        // If standard structure doesn't exist, try alternate: target_name/date/LIGHT/filename
        if !source_path.exists() {
            // Extract target name from the original path
            let mut target_name = None;
            let mut date = None;
            
            // Look for the target name and date in the path
            for (i, part) in path_parts.iter().enumerate() {
                // Check if this looks like a date
                if part.len() == 10 && part.chars().nth(4) == Some('-') && part.chars().nth(7) == Some('-') {
                    date = Some(*part);
                    // The target name is likely before the date
                    if i > 0 {
                        target_name = Some(path_parts[i-1]);
                    }
                }
            }
            
            // Also check the filename for a date if we didn't find one
            if date.is_none() && filename.len() > 10 {
                let potential_date = &filename[0..10];
                if potential_date.chars().nth(4) == Some('-') && potential_date.chars().nth(7) == Some('-') {
                    date = Some(potential_date);
                    // If we only found date in filename, target name should be from path
                    if target_name.is_none() {
                        // Find the last non-LIGHT, non-date directory in path
                        for part in path_parts.iter().rev() {
                            if *part != "LIGHT" && *part != "rejected" && *part != *filename 
                                && !(part.len() == 10 && part.chars().nth(4) == Some('-')) {
                                target_name = Some(*part);
                                break;
                            }
                        }
                    }
                }
            }
            
            if let (Some(target), Some(date_str)) = (target_name, date) {
                let alt_source_path = PathBuf::from(base_dir)
                    .join(target)
                    .join(date_str)
                    .join("LIGHT")
                    .join(filename);
                
                let alt_rejected_path = PathBuf::from(base_dir)
                    .join(target)
                    .join(date_str)
                    .join("LIGHT")
                    .join("rejected")
                    .join(filename);
                
                if alt_source_path.exists() || alt_rejected_path.exists() {
                    source_path = alt_source_path;
                    dest_path = PathBuf::from(base_dir)
                        .join(target)
                        .join(date_str)
                        .join("LIGHT_REJECT")
                        .join(filename);
                    found_structure = true;
                }
            }
        }
        
        // Skip if we couldn't determine a valid structure
        if !found_structure || source_path.as_os_str().is_empty() {
            println!("  SKIP: Could not determine file structure for: {}", original_path);
            continue;
        }
        
        // Check if source file exists in either LIGHT or LIGHT/rejected
        let mut actual_source_path = source_path.clone();
        let rejected_source_path = if let Some(parent) = source_path.parent() {
            parent.join("rejected").join(filename)
        } else {
            source_path.clone()
        };
        
        if !source_path.exists() {
            if rejected_source_path.exists() {
                // File is in the rejected subdirectory, we'll move it from there
                actual_source_path = rejected_source_path;
            } else {
                println!("  NOT FOUND: {} (also checked {})", source_path.display(), rejected_source_path.display());
                not_found_count += 1;
                continue;
            }
        }
        
        // Create destination directory if needed
        let dest_dir = dest_path.parent().unwrap();
        
        if dry_run {
            println!("  WOULD MOVE: {} -> {}", 
                     actual_source_path.display(), 
                     dest_path.display());
            println!("    Reason: {}", image.reject_reason.as_deref().unwrap_or("No reason given"));
            moved_count += 1;
        } else {
            // Create destination directory
            match fs::create_dir_all(dest_dir) {
                Ok(_) => {
                    // Move the file
                    match fs::rename(&actual_source_path, &dest_path) {
                        Ok(_) => {
                            println!("  MOVED: {} -> {}", 
                                     actual_source_path.display(), 
                                     dest_path.display());
                            println!("    Reason: {}", image.reject_reason.as_deref().unwrap_or("No reason given"));
                            moved_count += 1;
                        },
                        Err(e) => {
                            println!("  ERROR moving {}: {}", actual_source_path.display(), e);
                            error_count += 1;
                        }
                    }
                },
                Err(e) => {
                    println!("  ERROR creating directory {}: {}", dest_dir.display(), e);
                    error_count += 1;
                }
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