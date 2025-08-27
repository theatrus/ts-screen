use crate::db::Database;
use crate::utils::truncate_string;
use anyhow::Result;
use rusqlite::Connection;

pub fn list_targets(conn: &Connection, project_identifier: &str) -> Result<()> {
    let db = Database::new(conn);

    let project_id: i32 = if let Ok(id) = project_identifier.parse::<i32>() {
        id
    } else {
        db.find_project_id_by_name(project_identifier)?
    };

    let targets = db.get_targets_with_stats(project_id)?;

    println!(
        "{:<10} {:<30} {:<10} {:<15} {:<15} {:<10} {:<10} {:<10}",
        "ID", "Name", "Active", "RA", "Dec", "Images", "Accepted", "Rejected"
    );
    println!("{:-<120}", "");

    for (target, image_count, accepted_count, rejected_count) in targets {
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
