use crate::models::Project;
use crate::utils::truncate_string;
use anyhow::Result;
use rusqlite::Connection;

pub fn list_projects(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT Id, profileId, name, description 
         FROM project 
         ORDER BY name",
    )?;

    let project_iter = stmt.query_map([], |row| {
        Ok(Project {
            id: row.get(0)?,
            profile_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
        })
    })?;

    println!(
        "{:<10} {:<30} {:<20} {:<40}",
        "ID", "Name", "Profile ID", "Description"
    );
    println!("{:-<100}", "");

    for project in project_iter {
        let project = project?;
        println!(
            "{:<10} {:<30} {:<20} {:<40}",
            project.id,
            truncate_string(&project.name, 30),
            truncate_string(&project.profile_id, 20),
            truncate_string(&project.description.unwrap_or_else(|| "".to_string()), 40)
        );
    }

    Ok(())
}
