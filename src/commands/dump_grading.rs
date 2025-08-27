use crate::models::{AcquiredImage, GradingStatus};
use crate::utils::{extract_filename, truncate_string};
use anyhow::Result;
use rusqlite::Connection;

pub fn dump_grading_results(
    conn: &Connection,
    status_filter: Option<String>,
    project_filter: Option<String>,
    target_filter: Option<String>,
    format: &str,
) -> Result<()> {
    let mut query = String::from(
        "SELECT ai.Id, ai.projectId, ai.targetId, ai.acquireddate, ai.filtername, 
                ai.gradingStatus, ai.metadata, ai.rejectreason, ai.profileId,
                p.name as project_name, t.name as target_name
         FROM acquiredimage ai
         JOIN project p ON ai.projectId = p.Id
         JOIN target t ON ai.targetId = t.Id
         WHERE 1=1",
    );

    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(status) = &status_filter {
        let status_value = match status.to_lowercase().as_str() {
            "pending" => 0,
            "accepted" => 1,
            "rejected" => 2,
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid status: {}. Use pending, accepted, or rejected",
                    status
                ))
            }
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

    let mut results = Vec::new();
    for image in image_iter {
        results.push(image?);
    }

    match format {
        "json" => output_json(&results)?,
        "csv" => output_csv(&results)?,
        _ => output_table(&results)?,
    }

    Ok(())
}

fn output_table(results: &[(AcquiredImage, String, String)]) -> Result<()> {
    println!(
        "{:<10} {:<50} {:<20} {:<20} {:<15} {:<10} {:<16} {:<20}",
        "ID", "Filename", "Project", "Target", "Filter", "Status", "Date", "Reject Reason"
    );
    println!("{:-<180}", "");

    for (image, project_name, target_name) in results {
        let date_str = image
            .acquired_date
            .and_then(|d| chrono::DateTime::from_timestamp(d, 0))
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "No date".to_string());

        let filename = extract_filename(&image.metadata).unwrap_or_else(|| "Unknown".to_string());

        println!(
            "{:<10} {:<50} {:<20} {:<20} {:<15} {:<10} {:<16} {:<20}",
            image.id,
            truncate_string(&filename, 50),
            truncate_string(project_name, 20),
            truncate_string(target_name, 20),
            truncate_string(&image.filter_name, 15),
            GradingStatus::from_i32(image.grading_status),
            date_str,
            image.reject_reason.as_deref().unwrap_or("")
        );
    }

    println!("\nTotal: {} images", results.len());
    Ok(())
}

fn output_json(results: &[(AcquiredImage, String, String)]) -> Result<()> {
    let json_results: Vec<serde_json::Value> = results
        .iter()
        .map(|(image, project, target)| {
            let filename =
                extract_filename(&image.metadata).unwrap_or_else(|| "Unknown".to_string());

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
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&json_results)?);
    Ok(())
}

fn output_csv(results: &[(AcquiredImage, String, String)]) -> Result<()> {
    println!("id,filename,project_name,target_name,filter_name,grading_status,acquired_date,reject_reason");

    for (image, project_name, target_name) in results {
        let date_str = image
            .acquired_date
            .map(|d| d.to_string())
            .unwrap_or_else(|| "".to_string());

        let filename = extract_filename(&image.metadata).unwrap_or_else(|| "Unknown".to_string());

        println!(
            "{},{},{},{},{},{},{},{}",
            image.id,
            filename,
            project_name,
            target_name,
            image.filter_name,
            GradingStatus::from_i32(image.grading_status),
            date_str,
            image.reject_reason.as_deref().unwrap_or("")
        );
    }
    Ok(())
}
