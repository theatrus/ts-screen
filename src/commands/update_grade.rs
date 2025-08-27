use crate::db::Database;
use crate::models::GradingStatus;
use anyhow::Result;
use rusqlite::Connection;

pub fn update_grade(
    conn: &Connection,
    image_id: i32,
    status_str: &str,
    reason: Option<String>,
) -> Result<()> {
    let db = Database::new(conn);

    // Parse status
    let status = match status_str.to_lowercase().as_str() {
        "pending" => GradingStatus::Pending,
        "accepted" => GradingStatus::Accepted,
        "rejected" => GradingStatus::Rejected,
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid status: {}. Use pending, accepted, or rejected",
                status_str
            ))
        }
    };

    // Validate reason for rejected status
    if matches!(status, GradingStatus::Rejected) && reason.is_none() {
        return Err(anyhow::anyhow!(
            "Rejection reason is required when setting status to rejected"
        ));
    }

    // Update the grading status
    db.update_grading_status(image_id, status, reason.as_deref())?;

    println!(
        "Successfully updated image {} to status: {}",
        image_id, status_str
    );
    if let Some(r) = reason {
        println!("Rejection reason: {}", r);
    }

    Ok(())
}
