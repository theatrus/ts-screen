use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

use crate::db::Database;
use crate::models::GradingStatus;
use crate::server::api::*;
use crate::server::state::AppState;

pub async fn list_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<ProjectResponse>>>, AppError> {
    let conn = state.db();
    let conn = conn.lock().map_err(|_| AppError::DatabaseError)?;
    let db = Database::new(&conn);

    let projects = db.get_all_projects().map_err(|_| AppError::DatabaseError)?;

    let response: Vec<ProjectResponse> = projects
        .into_iter()
        .map(|p| ProjectResponse {
            id: p.id,
            name: p.name,
            description: p.description,
        })
        .collect();

    Ok(Json(ApiResponse::success(response)))
}

pub async fn list_targets(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<i32>,
) -> Result<Json<ApiResponse<Vec<TargetResponse>>>, AppError> {
    let conn = state.db();
    let conn = conn.lock().map_err(|_| AppError::DatabaseError)?;
    let db = Database::new(&conn);

    let targets = db
        .get_targets_with_stats(project_id)
        .map_err(|_| AppError::DatabaseError)?;

    let response: Vec<TargetResponse> = targets
        .into_iter()
        .map(|(target, img_count, accepted, rejected)| TargetResponse {
            id: target.id,
            name: target.name,
            ra: target.ra,
            dec: target.dec,
            active: target.active,
            image_count: img_count,
            accepted_count: accepted,
            rejected_count: rejected,
        })
        .collect();

    Ok(Json(ApiResponse::success(response)))
}

pub async fn get_images(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ImageQuery>,
) -> Result<Json<ApiResponse<Vec<ImageResponse>>>, AppError> {
    let conn = state.db();
    let conn = conn.lock().map_err(|_| AppError::DatabaseError)?;
    let db = Database::new(&conn);

    // Convert status string to GradingStatus enum
    let status_filter = params.status.as_ref().and_then(|s| match s.as_str() {
        "pending" => Some(GradingStatus::Pending),
        "accepted" => Some(GradingStatus::Accepted),
        "rejected" => Some(GradingStatus::Rejected),
        _ => None,
    });

    // For now, we'll use None for project/target filters since we have IDs
    let images = db
        .query_images(status_filter, None, None, None)
        .map_err(|_| AppError::DatabaseError)?;

    // Filter by project_id and target_id if provided
    let filtered_images: Vec<_> = images
        .into_iter()
        .filter(|(img, _, _)| {
            params.project_id.is_none_or(|id| img.project_id == id)
                && params.target_id.is_none_or(|id| img.target_id == id)
        })
        .collect();

    // Apply limit and offset
    let offset = params.offset.unwrap_or(0) as usize;
    let limit = params.limit.unwrap_or(100) as usize;

    let response: Vec<ImageResponse> = filtered_images
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(img, proj_name, target_name)| {
            let metadata: serde_json::Value = serde_json::from_str(&img.metadata)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            ImageResponse {
                id: img.id,
                project_id: img.project_id,
                project_name: proj_name,
                target_id: img.target_id,
                target_name,
                acquired_date: img.acquired_date,
                filter_name: img.filter_name,
                grading_status: img.grading_status,
                reject_reason: img.reject_reason,
                metadata,
            }
        })
        .collect();

    Ok(Json(ApiResponse::success(response)))
}

pub async fn get_image(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<i32>,
) -> Result<Json<ApiResponse<ImageResponse>>, AppError> {
    let conn = state.db();
    let conn = conn.lock().map_err(|_| AppError::DatabaseError)?;
    let db = Database::new(&conn);

    let images = db
        .get_images_by_ids(&[image_id])
        .map_err(|_| AppError::DatabaseError)?;

    let image = images.into_iter().next().ok_or(AppError::NotFound)?;

    // Get project and target names
    let all_images = db
        .query_images(None, None, None, None)
        .map_err(|_| AppError::DatabaseError)?;

    let (_, proj_name, target_name) = all_images
        .into_iter()
        .find(|(img, _, _)| img.id == image_id)
        .ok_or(AppError::NotFound)?;

    let metadata: serde_json::Value = serde_json::from_str(&image.metadata)
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let response = ImageResponse {
        id: image.id,
        project_id: image.project_id,
        project_name: proj_name,
        target_id: image.target_id,
        target_name,
        acquired_date: image.acquired_date,
        filter_name: image.filter_name,
        grading_status: image.grading_status,
        reject_reason: image.reject_reason,
        metadata,
    };

    Ok(Json(ApiResponse::success(response)))
}

pub async fn update_image_grade(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<i32>,
    Json(request): Json<UpdateGradeRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let conn = state.db();
    let conn = conn.lock().map_err(|_| AppError::DatabaseError)?;
    let db = Database::new(&conn);

    let status = match request.status.as_str() {
        "pending" => GradingStatus::Pending,
        "accepted" => GradingStatus::Accepted,
        "rejected" => GradingStatus::Rejected,
        _ => return Err(AppError::BadRequest("Invalid status".to_string())),
    };

    db.update_grading_status(image_id, status, request.reason.as_deref())
        .map_err(|_| AppError::DatabaseError)?;

    Ok(Json(ApiResponse::success(())))
}

use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

// Image preview endpoint
#[axum::debug_handler]
pub async fn get_image_preview(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<i32>,
    Query(options): Query<PreviewOptions>,
) -> Result<impl IntoResponse, AppError> {
    use crate::image_analysis::FitsImage;
    use crate::server::cache::CacheManager;

    // Get image metadata from database
    let (image, file_only, target_name) = {
        let conn = state.db();
        let conn = conn.lock().map_err(|_| AppError::DatabaseError)?;
        let db = Database::new(&conn);

        // Get image metadata
        let images = db
            .get_images_by_ids(&[image_id])
            .map_err(|_| AppError::DatabaseError)?;

        let image = images.into_iter().next().ok_or(AppError::NotFound)?;

        // Get target name
        let targets = db
            .get_targets_by_ids(&[image.target_id])
            .map_err(|_| AppError::DatabaseError)?;

        let target = targets.into_iter().next().ok_or(AppError::NotFound)?;
        let target_name = target.name.clone();

        // Extract filename from metadata
        let metadata: serde_json::Value = serde_json::from_str(&image.metadata)
            .map_err(|_| AppError::BadRequest("Invalid metadata".to_string()))?;

        let filename = metadata["FileName"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("No filename in metadata".to_string()))?;

        // Extract just the filename from the full path
        let file_only = filename
            .split(&['\\', '/'][..])
            .next_back()
            .ok_or_else(|| AppError::BadRequest("Invalid filename format".to_string()))?
            .to_string();

        (image, file_only, target_name)
    }; // Connection is dropped here

    // Determine cache parameters
    let size = options.size.as_deref().unwrap_or("screen");
    let stretch = options.stretch.unwrap_or(true);
    let midtone = options.midtone.unwrap_or(0.2);
    let shadow = options.shadow.unwrap_or(-2.8);

    // Create cache key
    let cache_key = format!(
        "{}_{}_{}_{}_{}",
        image_id,
        size,
        if stretch { "stretched" } else { "linear" },
        (midtone * 1000.0) as i32,
        (shadow * 1000.0) as i32
    );

    let cache_manager = CacheManager::new(state.cache_dir.clone());
    cache_manager
        .ensure_category_dir("previews")
        .map_err(|e| AppError::InternalError(format!("Failed to create cache directory: {}", e)))?;
    let cache_path = cache_manager.get_cached_path("previews", &cache_key, "png");

    // Check if cached version exists
    if cache_manager.is_cached(&cache_path) {
        // Serve from cache
        let mut file = File::open(&cache_path)
            .await
            .map_err(|_| AppError::InternalError("Failed to read cache".to_string()))?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .await
            .map_err(|_| AppError::InternalError("Failed to read file".to_string()))?;

        return Ok((
            StatusCode::OK,
            [
                (CONTENT_TYPE, "image/png"),
                (CACHE_CONTROL, "max-age=86400"), // Cache for 1 day
            ],
            buffer,
        ));
    }

    // Find the FITS file
    let fits_path = find_fits_file(&state, &image, &target_name, &file_only)?;

    // Load FITS file (just to verify it exists and is valid)
    let _fits = FitsImage::from_file(&fits_path)
        .map_err(|e| AppError::InternalError(format!("Failed to load FITS: {}", e)))?;

    // Determine target size
    let max_dimensions = match size {
        "large" => Some((2000, 2000)),
        "screen" => Some((1200, 1200)),
        _ => None, // No resize for other sizes
    };

    // Use the existing stretch_to_png function to write directly to cache
    use crate::commands::stretch_to_png::stretch_to_png_with_resize;

    // Create a temporary path for the cache file
    let cache_path_str = cache_path.to_string_lossy().to_string();

    // Generate the stretched PNG with optional resizing
    stretch_to_png_with_resize(
        &fits_path.to_string_lossy(),
        Some(cache_path_str.clone()),
        midtone,
        shadow,
        false, // logarithmic
        false, // invert
        max_dimensions,
    )
    .map_err(|e| AppError::InternalError(format!("Failed to generate preview: {}", e)))?;

    // Read the file back into memory
    let png_buffer = tokio::fs::read(&cache_path)
        .await
        .map_err(|_| AppError::InternalError("Failed to read generated PNG".to_string()))?;

    Ok((
        StatusCode::OK,
        [
            (CONTENT_TYPE, "image/png"),
            (CACHE_CONTROL, "max-age=86400"), // Cache for 1 day
        ],
        png_buffer,
    ))
}

// Helper function to find FITS file
fn find_fits_file(
    state: &AppState,
    image: &crate::models::AcquiredImage,
    target_name: &str,
    filename: &str,
) -> Result<std::path::PathBuf, AppError> {
    use crate::commands::filter_rejected::{find_file_recursive, get_possible_paths};

    // Extract date from acquired_date
    let acquired_date = image
        .acquired_date
        .and_then(|d| chrono::DateTime::from_timestamp(d, 0))
        .ok_or_else(|| AppError::BadRequest("Invalid date".to_string()))?;

    let date_str = acquired_date.format("%Y-%m-%d").to_string();

    // Try to find the file in different possible locations
    let possible_paths = get_possible_paths(
        &state.image_dir.to_string_lossy(),
        &date_str,
        target_name,
        filename,
    );

    for path in &possible_paths {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    // Try recursive search as fallback
    match find_file_recursive(&state.image_dir.to_string_lossy(), filename)
        .map_err(|e| AppError::InternalError(format!("Search failed: {}", e)))?
    {
        Some(path) => Ok(path),
        None => Err(AppError::NotFound),
    }
}

#[axum::debug_handler]
pub async fn get_image_stars(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<i32>,
) -> Result<Json<ApiResponse<StarDetectionResponse>>, AppError> {
    use crate::hocus_focus_star_detection::{detect_stars_hocus_focus, HocusFocusParams};
    use crate::image_analysis::FitsImage;
    use crate::psf_fitting::PSFType;
    use crate::server::cache::CacheManager;

    // Get image metadata from database
    let (image, file_only, target_name) = {
        let conn = state.db();
        let conn = conn.lock().map_err(|_| AppError::DatabaseError)?;
        let db = Database::new(&conn);

        let images = db
            .get_images_by_ids(&[image_id])
            .map_err(|_| AppError::DatabaseError)?;

        let image = images.into_iter().next().ok_or(AppError::NotFound)?;

        // Get target name
        let targets = db
            .get_targets_by_ids(&[image.target_id])
            .map_err(|_| AppError::DatabaseError)?;

        let target = targets.into_iter().next().ok_or(AppError::NotFound)?;
        let target_name = target.name.clone();

        let metadata: serde_json::Value = serde_json::from_str(&image.metadata)
            .map_err(|_| AppError::BadRequest("Invalid metadata".to_string()))?;

        let filename = metadata["FileName"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("No filename in metadata".to_string()))?;

        let file_only = filename
            .split(&['\\', '/'][..])
            .next_back()
            .ok_or_else(|| AppError::BadRequest("Invalid filename format".to_string()))?
            .to_string();

        (image, file_only, target_name)
    };

    // Create cache key for star detection results
    let cache_key = format!("stars_{}", image_id);
    let cache_manager = CacheManager::new(state.cache_dir.clone());
    cache_manager
        .ensure_category_dir("stars")
        .map_err(|e| AppError::InternalError(format!("Failed to create cache directory: {}", e)))?;
    let cache_path = cache_manager.get_cached_path("stars", &cache_key, "json");

    // Check if cached version exists
    if cache_manager.is_cached(&cache_path) {
        // Read from cache
        let cached_data = tokio::fs::read_to_string(&cache_path)
            .await
            .map_err(|_| AppError::InternalError("Failed to read cache".to_string()))?;

        let response: StarDetectionResponse = serde_json::from_str(&cached_data)
            .map_err(|_| AppError::InternalError("Invalid cached data".to_string()))?;

        return Ok(Json(ApiResponse::success(response)));
    }

    // Find and load the FITS file
    let fits_path = find_fits_file(&state, &image, &target_name, &file_only)?;
    let fits = FitsImage::from_file(&fits_path)
        .map_err(|e| AppError::InternalError(format!("Failed to load FITS: {}", e)))?;

    // Run star detection
    let params = HocusFocusParams {
        psf_type: PSFType::Moffat4,
        ..Default::default()
    };

    let detection_result = detect_stars_hocus_focus(&fits.data, fits.width, fits.height, &params);

    // Convert to API response format
    let stars: Vec<StarInfo> = detection_result
        .stars
        .iter()
        .map(|star| {
            let eccentricity = if let Some(psf) = &star.psf_model {
                psf.eccentricity
            } else {
                0.0
            };

            StarInfo {
                x: star.position.0,
                y: star.position.1,
                hfr: star.hfr,
                fwhm: star.fwhm,
                brightness: star.brightness,
                eccentricity,
            }
        })
        .collect();

    let response = StarDetectionResponse {
        detected_stars: detection_result.stars.len(),
        average_hfr: detection_result.average_hfr,
        average_fwhm: detection_result.average_fwhm,
        stars,
    };

    // Save to cache
    let cached_data = serde_json::to_string(&response)
        .map_err(|_| AppError::InternalError("Failed to serialize response".to_string()))?;

    tokio::fs::write(&cache_path, cached_data)
        .await
        .map_err(|_| AppError::InternalError("Failed to write cache".to_string()))?;

    Ok(Json(ApiResponse::success(response)))
}

#[axum::debug_handler]
pub async fn get_annotated_image(
    State(state): State<Arc<AppState>>,
    Path(image_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    use crate::hocus_focus_star_detection::{detect_stars_hocus_focus, HocusFocusParams};
    use crate::image_analysis::FitsImage;
    use crate::mtf_stretch::{stretch_image, StretchParameters};
    use crate::psf_fitting::PSFType;
    use crate::server::cache::CacheManager;
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::{ColorType, ImageBuffer, ImageEncoder, Rgb};
    use imageproc::drawing::{draw_filled_circle_mut, draw_hollow_circle_mut};

    // Get image metadata from database
    let (image, file_only, target_name) = {
        let conn = state.db();
        let conn = conn.lock().map_err(|_| AppError::DatabaseError)?;
        let db = Database::new(&conn);

        let images = db
            .get_images_by_ids(&[image_id])
            .map_err(|_| AppError::DatabaseError)?;

        let image = images.into_iter().next().ok_or(AppError::NotFound)?;

        // Get target name
        let targets = db
            .get_targets_by_ids(&[image.target_id])
            .map_err(|_| AppError::DatabaseError)?;
        
        let target = targets.into_iter().next().ok_or(AppError::NotFound)?;
        let target_name = target.name.clone();

        let metadata: serde_json::Value = serde_json::from_str(&image.metadata)
            .map_err(|_| AppError::BadRequest("Invalid metadata".to_string()))?;

        let filename = metadata["FileName"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("No filename in metadata".to_string()))?;

        let file_only = filename
            .split(&['\\', '/'][..])
            .next_back()
            .ok_or_else(|| AppError::BadRequest("Invalid filename format".to_string()))?
            .to_string();

        (image, file_only, target_name)
    };

    // Create cache key for annotated image
    let cache_key = format!("annotated_{}", image_id);
    let cache_manager = CacheManager::new(state.cache_dir.clone());
    cache_manager
        .ensure_category_dir("annotated")
        .map_err(|e| AppError::InternalError(format!("Failed to create cache directory: {}", e)))?;
    let cache_path = cache_manager.get_cached_path("annotated", &cache_key, "png");

    // Check if cached version exists
    if cache_manager.is_cached(&cache_path) {
        // Serve from cache
        let mut file = File::open(&cache_path)
            .await
            .map_err(|_| AppError::InternalError("Failed to read cache".to_string()))?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .await
            .map_err(|_| AppError::InternalError("Failed to read file".to_string()))?;

        return Ok((
            StatusCode::OK,
            [
                (CONTENT_TYPE, "image/png"),
                (CACHE_CONTROL, "max-age=86400"), // Cache for 1 day
            ],
            buffer,
        ));
    }

    // Find and load the FITS file
    let fits_path = find_fits_file(&state, &image, &target_name, &file_only)?;
    let fits = FitsImage::from_file(&fits_path)
        .map_err(|e| AppError::InternalError(format!("Failed to load FITS: {}", e)))?;

    // Calculate image statistics
    let stats = fits.calculate_basic_statistics();

    // Apply MTF stretch
    let stretch_params = StretchParameters {
        factor: 0.2,
        black_clipping: -2.8,
    };

    let stretched = stretch_image(
        &fits.data,
        &stats,
        stretch_params.factor,
        stretch_params.black_clipping,
    );

    // Detect stars using HocusFocus
    let params = HocusFocusParams {
        psf_type: PSFType::None,
        ..Default::default()
    };

    let detection_result = detect_stars_hocus_focus(&fits.data, fits.width, fits.height, &params);
    
    // Sort stars by HFR (smallest first - best focus) and take top 100
    let mut stars: Vec<_> = detection_result.stars.iter()
        .map(|s| (s.position.0, s.position.1, s.hfr))
        .collect();
    stars.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
    let stars_to_annotate: Vec<_> = stars.into_iter().take(100).collect();

    // Convert stretched 16-bit data to 8-bit RGB
    let mut rgb_image = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(fits.width as u32, fits.height as u32);

    for (x, y, pixel) in rgb_image.enumerate_pixels_mut() {
        let idx = y as usize * fits.width + x as usize;
        let value = (stretched[idx] >> 8) as u8; // Convert 16-bit to 8-bit
        *pixel = Rgb([value, value, value]); // Grayscale to RGB
    }

    // Draw circles around detected stars (yellow color)
    let color = Rgb([255, 255, 0]);

    for (x, y, hfr) in &stars_to_annotate {
        // Calculate circle radius based on HFR
        let radius = (hfr * 2.5).max(5.0) as i32;

        // Draw hollow circle
        draw_hollow_circle_mut(&mut rgb_image, (*x as i32, *y as i32), radius, color);

        // For very small stars, also draw a filled center point
        if radius < 8 {
            draw_filled_circle_mut(&mut rgb_image, (*x as i32, *y as i32), 1, color);
        }
    }

    // Save to cache
    let cache_file = std::fs::File::create(&cache_path)
        .map_err(|_| AppError::InternalError("Failed to create cache file".to_string()))?;
    let writer = std::io::BufWriter::new(cache_file);

    // Create PNG encoder with best compression
    let encoder = PngEncoder::new_with_quality(writer, CompressionType::Best, FilterType::Adaptive);

    // Write the image data
    encoder
        .write_image(
            &rgb_image,
            fits.width as u32,
            fits.height as u32,
            ColorType::Rgb8.into(),
        )
        .map_err(|_| AppError::InternalError("Failed to write PNG".to_string()))?;

    // Read the file back into memory
    let png_buffer = tokio::fs::read(&cache_path)
        .await
        .map_err(|_| AppError::InternalError("Failed to read generated PNG".to_string()))?;

    Ok((
        StatusCode::OK,
        [
            (CONTENT_TYPE, "image/png"),
            (CACHE_CONTROL, "max-age=86400"), // Cache for 1 day
        ],
        png_buffer,
    ))
}

pub async fn get_psf_visualization(
    State(_state): State<Arc<AppState>>,
    Path(_image_id): Path<i32>,
) -> Result<Response, AppError> {
    // TODO: Implement PSF visualization
    Err(AppError::NotImplemented)
}

// Error handling
#[derive(Debug)]
pub enum AppError {
    NotFound,
    DatabaseError,
    BadRequest(String),
    InternalError(String),
    NotImplemented,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Resource not found"),
            AppError::DatabaseError => (StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
            AppError::BadRequest(msg) => {
                return (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::error(msg)))
                    .into_response()
            }
            AppError::InternalError(msg) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<()>::error(msg)),
                )
                    .into_response()
            }
            AppError::NotImplemented => (StatusCode::NOT_IMPLEMENTED, "Not implemented yet"),
        };

        (
            status,
            Json(ApiResponse::<()>::error(error_message.to_string())),
        )
            .into_response()
    }
}
