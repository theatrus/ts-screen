use crate::db::Database;
use crate::utils::truncate_string;
use anyhow::Result;
use rusqlite::Connection;

pub fn list_projects(conn: &Connection) -> Result<()> {
    let db = Database::new(conn);
    let projects = db.get_all_projects()?;

    println!(
        "{:<10} {:<30} {:<20} {:<40}",
        "ID", "Name", "Profile ID", "Description"
    );
    println!("{:-<100}", "");

    for project in projects {
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
