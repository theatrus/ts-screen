use clap::{Parser, Subcommand};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use anyhow::Context;

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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    let conn = Connection::open(&cli.database)
        .with_context(|| format!("Failed to open database: {}", cli.database))?;
    
    match cli.command {
        Commands::DumpGrading { status, project, target, format } => {
            dump_grading_results(&conn, status, project, target, &format)?;
        },
        Commands::ListProjects => {
            list_projects(&conn)?;
        },
        Commands::ListTargets { project } => {
            list_targets(&conn, &project)?;
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

fn output_table(results: &[(AcquiredImage, String, String)]) -> anyhow::Result<()> {
    println!("{:<10} {:<20} {:<20} {:<15} {:<10} {:<10} {:<30}",
             "ID", "Project", "Target", "Filter", "Status", "Date", "Reject Reason");
    println!("{:-<120}", "");
    
    for (image, project_name, target_name) in results {
        let date_str = image.acquired_date
            .and_then(|d| chrono::DateTime::from_timestamp(d, 0))
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "No date".to_string());
        
        println!("{:<10} {:<20} {:<20} {:<15} {:<10} {:<10} {:<30}",
                 image.id,
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
        serde_json::json!({
            "id": image.id,
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
    println!("id,project_name,target_name,filter_name,grading_status,acquired_date,reject_reason");
    
    for (image, project_name, target_name) in results {
        let date_str = image.acquired_date
            .map(|d| d.to_string())
            .unwrap_or_else(|| "".to_string());
        
        println!("{},{},{},{},{},{},{}",
                 image.id,
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