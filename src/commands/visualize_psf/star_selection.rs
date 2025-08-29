/// Star selection strategies for PSF visualization

use crate::hocus_focus_star_detection::HocusFocusStar;

pub enum SelectionStrategy {
    /// Top N stars by specified metric
    TopN { n: usize, metric: SortMetric },
    /// Stars from five regions (four quadrants + center)
    FiveRegions { per_region: usize },
    /// Stars with diverse quality scores
    QualityRange { per_tier: usize },
    /// Custom selection based on criteria
    Custom { min_hfr: Option<f64>, max_hfr: Option<f64>, min_r2: Option<f64> },
}

pub enum SortMetric {
    HFR,
    R2,
    Brightness,
}

/// Select stars based on the given strategy
pub fn select_stars(
    stars: Vec<HocusFocusStar>,
    strategy: &SelectionStrategy,
    image_width: usize,
    image_height: usize,
) -> Vec<HocusFocusStar> {
    match strategy {
        SelectionStrategy::TopN { n, metric } => {
            select_top_n(stars, *n, metric)
        }
        SelectionStrategy::FiveRegions { per_region } => {
            select_five_regions(stars, *per_region, image_width, image_height)
        }
        SelectionStrategy::QualityRange { per_tier } => {
            select_quality_range(stars, *per_tier)
        }
        SelectionStrategy::Custom { min_hfr, max_hfr, min_r2 } => {
            select_custom(stars, *min_hfr, *max_hfr, *min_r2)
        }
    }
}

fn select_top_n(mut stars: Vec<HocusFocusStar>, n: usize, metric: &SortMetric) -> Vec<HocusFocusStar> {
    // Sort by the specified metric
    match metric {
        SortMetric::HFR => stars.sort_by(|a, b| a.hfr.partial_cmp(&b.hfr).unwrap()),
        SortMetric::R2 => stars.sort_by(|a, b| {
            let r2_a = a.psf_model.as_ref().map(|m| m.r_squared).unwrap_or(0.0);
            let r2_b = b.psf_model.as_ref().map(|m| m.r_squared).unwrap_or(0.0);
            r2_b.partial_cmp(&r2_a).unwrap() // Higher R² first
        }),
        SortMetric::Brightness => stars.sort_by(|a, b| b.brightness.partial_cmp(&a.brightness).unwrap()),
    }
    
    stars.into_iter().take(n).collect()
}

fn select_five_regions(
    stars: Vec<HocusFocusStar>,
    per_region: usize,
    image_width: usize,
    image_height: usize,
) -> Vec<HocusFocusStar> {
    let center_x = image_width as f64 / 2.0;
    let center_y = image_height as f64 / 2.0;
    
    // Define regions
    let mut top_left = Vec::new();
    let mut top_right = Vec::new();
    let mut bottom_left = Vec::new();
    let mut bottom_right = Vec::new();
    let mut center = Vec::new();
    
    // Center region is within 25% of image dimensions from center
    let center_radius_x = image_width as f64 * 0.25;
    let center_radius_y = image_height as f64 * 0.25;
    
    // Categorize stars by region
    for star in stars {
        let (x, y) = star.position;
        
        // Check if in center region
        if (x - center_x).abs() < center_radius_x && (y - center_y).abs() < center_radius_y {
            center.push(star);
        } else if x < center_x && y < center_y {
            top_left.push(star);
        } else if x >= center_x && y < center_y {
            top_right.push(star);
        } else if x < center_x && y >= center_y {
            bottom_left.push(star);
        } else {
            bottom_right.push(star);
        }
    }
    
    // Sort each region by HFR (best first)
    let sort_by_hfr = |stars: &mut Vec<HocusFocusStar>| {
        stars.sort_by(|a, b| a.hfr.partial_cmp(&b.hfr).unwrap());
    };
    
    sort_by_hfr(&mut top_left);
    sort_by_hfr(&mut top_right);
    sort_by_hfr(&mut bottom_left);
    sort_by_hfr(&mut bottom_right);
    sort_by_hfr(&mut center);
    
    // Select top stars from each region
    let mut selected = Vec::new();
    selected.extend(top_left.into_iter().take(per_region));
    selected.extend(top_right.into_iter().take(per_region));
    selected.extend(bottom_left.into_iter().take(per_region));
    selected.extend(bottom_right.into_iter().take(per_region));
    selected.extend(center.into_iter().take(per_region));
    
    selected
}

fn select_quality_range(stars: Vec<HocusFocusStar>, per_tier: usize) -> Vec<HocusFocusStar> {
    // Filter stars with PSF models
    let mut stars_with_psf: Vec<_> = stars.into_iter()
        .filter(|s| s.psf_model.is_some())
        .collect();
    
    if stars_with_psf.is_empty() {
        return Vec::new();
    }
    
    // Sort by R² value
    stars_with_psf.sort_by(|a, b| {
        let r2_a = a.psf_model.as_ref().unwrap().r_squared;
        let r2_b = b.psf_model.as_ref().unwrap().r_squared;
        r2_b.partial_cmp(&r2_a).unwrap()
    });
    
    let total = stars_with_psf.len();
    let mut selected = Vec::new();
    
    // Define quality tiers
    let tiers = [
        (0.9, 1.0, "Excellent"),    // R² > 0.9
        (0.7, 0.9, "Good"),         // 0.7 < R² <= 0.9
        (0.5, 0.7, "Fair"),         // 0.5 < R² <= 0.7
        (0.0, 0.5, "Poor"),         // R² <= 0.5
    ];
    
    for (min_r2, max_r2, _name) in &tiers {
        let tier_stars: Vec<_> = stars_with_psf.iter()
            .filter(|s| {
                let r2 = s.psf_model.as_ref().unwrap().r_squared;
                r2 > *min_r2 && r2 <= *max_r2
            })
            .take(per_tier)
            .cloned()
            .collect();
        
        selected.extend(tier_stars);
    }
    
    // If we don't have enough from tiers, add some from the top
    if selected.len() < per_tier * 4 {
        let remaining = per_tier * 4 - selected.len();
        let additional: Vec<_> = stars_with_psf.into_iter()
            .filter(|s| !selected.iter().any(|sel| sel.position == s.position))
            .take(remaining)
            .collect();
        selected.extend(additional);
    }
    
    selected
}

fn select_custom(
    stars: Vec<HocusFocusStar>,
    min_hfr: Option<f64>,
    max_hfr: Option<f64>,
    min_r2: Option<f64>,
) -> Vec<HocusFocusStar> {
    stars.into_iter()
        .filter(|s| {
            // HFR filter
            if let Some(min) = min_hfr {
                if s.hfr < min {
                    return false;
                }
            }
            if let Some(max) = max_hfr {
                if s.hfr > max {
                    return false;
                }
            }
            
            // R² filter
            if let Some(min) = min_r2 {
                if let Some(psf) = &s.psf_model {
                    if psf.r_squared < min {
                        return false;
                    }
                } else {
                    return false; // No PSF model
                }
            }
            
            true
        })
        .collect()
}