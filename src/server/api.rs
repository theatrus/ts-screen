use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TargetResponse {
    pub id: i32,
    pub name: String,
    pub ra: Option<f64>,
    pub dec: Option<f64>,
    pub active: bool,
    pub image_count: i32,
    pub accepted_count: i32,
    pub rejected_count: i32,
}

#[derive(Debug, Serialize)]
pub struct ImageResponse {
    pub id: i32,
    pub project_id: i32,
    pub project_name: String,
    pub target_id: i32,
    pub target_name: String,
    pub acquired_date: Option<i64>,
    pub filter_name: String,
    pub grading_status: i32,
    pub reject_reason: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ImageQuery {
    pub project_id: Option<i32>,
    pub target_id: Option<i32>,
    pub status: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGradeRequest {
    pub status: String, // "accepted", "rejected", "pending"
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StarDetectionResponse {
    pub detected_stars: usize,
    pub average_hfr: f64,
    pub average_fwhm: f64,
    pub stars: Vec<StarInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StarInfo {
    pub x: f64,
    pub y: f64,
    pub hfr: f64,
    pub fwhm: f64,
    pub brightness: f64,
    pub eccentricity: f64,
}

#[derive(Debug, Deserialize)]
pub struct PreviewOptions {
    pub size: Option<String>, // "screen" or "large"
    pub stretch: Option<bool>,
    pub midtone: Option<f64>,
    pub shadow: Option<f64>,
}
