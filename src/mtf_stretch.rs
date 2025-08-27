/// Midtone Transfer Function (MTF) stretching implementation
/// Based on N.I.N.A.'s image stretching algorithm

use crate::image_analysis::ImageStatistics;

/// Apply MTF stretch to image data using N.I.N.A.'s algorithm
pub fn stretch_image(
    data: &[u16],
    statistics: &ImageStatistics,
    factor: f64,
    black_clipping: f64,
) -> Vec<u16> {
    // Calculate target histogram median percent from factor
    // N.I.N.A. default factor is 0.15
    let target_histogram_median_pct = factor;
    
    // Generate stretch mapping table
    let stretch_map = get_stretch_map(statistics, target_histogram_median_pct, black_clipping);
    
    // Apply mapping to all pixels
    data.iter()
        .map(|&pixel| stretch_map[pixel as usize])
        .collect()
}

/// Generate the stretch mapping table using N.I.N.A.'s algorithm
fn get_stretch_map(
    statistics: &ImageStatistics,
    target_histogram_median_pct: f64,
    shadows_clipping: f64,
) -> Vec<u16> {
    let mut map = vec![0u16; 65536]; // Full 16-bit range
    
    // Normalize median and MAD to 0-1 range
    let bit_depth = 16; // Assuming 16-bit for FITS files
    let normalized_median = normalize_u16(statistics.median as u16, bit_depth);
    let normalized_mad = calculate_mad(statistics) / 65535.0;
    
    let scale_factor = 1.4826; // MAD to sigma conversion factor
    
    let (shadows, midtones, highlights) = if normalized_median > 0.5 {
        // Image is inverted or overexposed
        let shadows = 0.0;
        let highlights = normalized_median - shadows_clipping * normalized_mad * scale_factor;
        let midtones = midtones_transfer_function(
            target_histogram_median_pct,
            1.0 - (highlights - normalized_median),
        );
        (shadows, midtones, highlights)
    } else {
        // Normal image
        let shadows = normalized_median + shadows_clipping * normalized_mad * scale_factor;
        let midtones = midtones_transfer_function(
            target_histogram_median_pct,
            normalized_median - shadows,
        );
        let highlights = 1.0;
        (shadows, midtones, highlights)
    };
    
    // Generate mapping for each possible pixel value
    for i in 0..map.len() {
        let value = normalize_u16(i as u16, bit_depth);
        let stretched = midtones_transfer_function(
            midtones,
            (1.0 - highlights + value - shadows).clamp(0.0, 1.0),
        );
        map[i] = denormalize_u16(stretched);
    }
    
    map
}

/// Calculate Median Absolute Deviation (MAD) from statistics
fn calculate_mad(statistics: &ImageStatistics) -> f64 {
    // For now, use a simple approximation based on standard deviation
    // In a full implementation, we'd calculate this properly from the image data
    // MAD ≈ 0.6745 * σ for normal distribution
    statistics.std_dev * 0.6745
}

/// Normalize 16-bit value to 0-1 range considering bit depth
fn normalize_u16(value: u16, bit_depth: u8) -> f64 {
    let max_val = (1u32 << bit_depth) - 1;
    value as f64 / max_val as f64
}

/// Denormalize 0-1 value back to 16-bit range
fn denormalize_u16(value: f64) -> u16 {
    (value.clamp(0.0, 1.0) * 65535.0).round() as u16
}

/// Midtones Transfer Function (MTF)
/// This is the key stretching function used by N.I.N.A.
fn midtones_transfer_function(midtones: f64, value: f64) -> f64 {
    if value == 0.0 {
        return 0.0;
    }
    if value == 1.0 {
        return 1.0;
    }
    if value == midtones {
        return 0.5;
    }
    
    // MTF formula
    if value < midtones {
        (value / midtones) / (2.0 * (1.0 - value / midtones) + 1.0)
    } else {
        let denominator = 2.0 * (1.0 - (value - midtones) / (1.0 - midtones)) + 1.0;
        if denominator == 0.0 {
            1.0
        } else {
            ((value - midtones) / (1.0 - midtones)) / denominator + 0.5
        }
    }
}

/// Configuration for MTF stretching matching N.I.N.A. defaults
pub struct StretchParameters {
    pub factor: f64,          // Target histogram median position (default: 0.15)
    pub black_clipping: f64,  // Shadow clipping in MAD units (default: -2.8)
}

impl Default for StretchParameters {
    fn default() -> Self {
        Self {
            factor: 0.2,          // N.I.N.A. default AutoStretchFactor
            black_clipping: -2.8, // N.I.N.A. default BlackClipping
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midtones_transfer_function() {
        // Test boundary conditions
        assert_eq!(midtones_transfer_function(0.5, 0.0), 0.0);
        assert_eq!(midtones_transfer_function(0.5, 1.0), 1.0);
        assert_eq!(midtones_transfer_function(0.5, 0.5), 0.5);
        
        // Test typical values
        let mtf = midtones_transfer_function(0.5, 0.25);
        assert!(mtf > 0.0 && mtf < 0.5);
        
        let mtf = midtones_transfer_function(0.5, 0.75);
        assert!(mtf > 0.5 && mtf < 1.0);
    }
    
    #[test]
    fn test_normalize_denormalize() {
        assert_eq!(normalize_u16(0, 16), 0.0);
        assert_eq!(normalize_u16(65535, 16), 1.0);
        assert_eq!(normalize_u16(32768, 16), 0.5);
        
        assert_eq!(denormalize_u16(0.0), 0);
        assert_eq!(denormalize_u16(1.0), 65535);
        assert_eq!(denormalize_u16(0.5), 32768);
    }
}