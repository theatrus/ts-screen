use crate::models::Target;
use crate::utils::truncate_string;
use anyhow::{Context, Result};
use rusqlite::Connection;

pub fn list_targets(conn: &Connection, project_identifier: &str) -> Result<()> {
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
         ORDER BY t.name",
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

    println!(
        "{:<10} {:<30} {:<10} {:<15} {:<15} {:<10} {:<10} {:<10}",
        "ID", "Name", "Active", "RA", "Dec", "Images", "Accepted", "Rejected"
    );
    println!("{:-<120}", "");

    for target in target_iter {
        let (target, image_count, accepted_count, rejected_count) = target?;
        println!(
            "{:<10} {:<30} {:<10} {:<15.6} {:<15.6} {:<10} {:<10} {:<10}",
            target.id,
            truncate_string(&target.name, 30),
            if target.active { "Yes" } else { "No" },
            target.ra.unwrap_or(0.0),
            target.dec.unwrap_or(0.0),
            image_count,
            accepted_count,
            rejected_count
        );
    }

    Ok(())
}
