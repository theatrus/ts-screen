use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: i32,
    pub profile_id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Target {
    pub id: i32,
    pub name: String,
    pub active: bool,
    pub ra: Option<f64>,
    pub dec: Option<f64>,
    pub project_id: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcquiredImage {
    pub id: i32,
    pub project_id: i32,
    pub target_id: i32,
    pub acquired_date: Option<i64>,
    pub filter_name: String,
    pub grading_status: i32,
    pub metadata: String,
    pub reject_reason: Option<String>,
    pub profile_id: Option<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum GradingStatus {
    Pending = 0,
    Accepted = 1,
    Rejected = 2,
}

impl GradingStatus {
    pub fn from_i32(value: i32) -> &'static str {
        match value {
            0 => "Pending",
            1 => "Accepted",
            2 => "Rejected",
            _ => "Unknown",
        }
    }
}