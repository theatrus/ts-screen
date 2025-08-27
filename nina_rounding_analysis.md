# N.I.N.A. Star Detection Pipeline - Rounding Strategy Analysis

## Summary

After analyzing the N.I.N.A. codebase, I've identified all uses of Math.Round, Math.Floor, and Math.Ceiling in the star detection and HFR calculation pipeline. The analysis reveals that N.I.N.A. uses the default .NET rounding behavior (banker's rounding) throughout, as no MidpointRounding parameter is specified anywhere in the codebase.

## Key Findings

### 1. Star Detection Pipeline (StarDetection.cs)

#### HFR Calculation
- **Line 144**: `double value = Math.Round(data.Value - SurroundingMean);`
  - Uses default banker's rounding (MidpointRounding.ToEven)
  - Rounds background-subtracted pixel values before HFR calculation
  - This is the only rounding operation in the core HFR calculation

#### Image Resize Calculations
- **Line 103**: `state._minStarSize = (int)Math.Floor(5 * state._resizefactor);`
  - Always rounds down for minimum star size
- **Line 109**: `state._maxStarSize = (int)Math.Ceiling(150 * state._resizefactor);`
  - Always rounds up for maximum star size
- **Line 281**: Rectangle calculations use Floor for position and Ceiling for dimensions:
  ```csharp
  var rect = new Rectangle(
      (int)Math.Floor(blob.Rectangle.X * state._inverseResizefactor),
      (int)Math.Floor(blob.Rectangle.Y * state._inverseResizefactor),
      (int)Math.Ceiling(blob.Rectangle.Width * state._inverseResizefactor),
      (int)Math.Ceiling(blob.Rectangle.Height * state._inverseResizefactor)
  );
  ```
- **Line 338**: `int minimumNumberOfPixels = (int)Math.Ceiling(Math.Max(state._originalBitmapSource.PixelWidth, state._originalBitmapSource.PixelHeight) / 1000d);`
  - Rounds up for minimum pixel count threshold

### 2. Detection Utility (DetectionUtility.cs)

#### Gaussian Kernel Generation
- **Line 43**: `int value = (int)Math.Round(LaplacianOfGaussianFunction(x, y, sigma));`
  - Uses default banker's rounding for kernel values

#### Image Cropping
- **Lines 25-28**: All use Math.Floor for crop rectangle calculations:
  ```csharp
  int xcoord = (int)Math.Floor((image.Width - image.Width * cropRatio) / 2d);
  int ycoord = (int)Math.Floor((image.Height - image.Height * cropRatio) / 2d);
  int width = (int)Math.Floor(image.Width * cropRatio);
  int height = (int)Math.Floor(image.Height * cropRatio);
  ```

#### Image Resizing
- **Line 55**: `new ResizeBicubic((int)Math.Floor(image.Width * resizeFactor), (int)Math.Floor(image.Height * resizeFactor))`
  - Always rounds down for resized dimensions

#### ROI Calculations
- **Lines 74-77, 87-90**: All use Math.Floor for ROI rectangle calculations

### 3. Fast Gaussian Blur (FastGaussianBlur.cs)

#### Blur Calculations
- **Lines 71, 76**: Box blur kernel size calculations:
  ```csharp
  var wl = (int)Math.Floor(wIdeal);
  var m = Math.Round(mIdeal);
  ```
- **Lines 101, 105, 109, 126, 135, 139**: All use Math.Floor for blur pixel value calculations:
  ```csharp
  dest[ti++] = (byte)Math.Floor(val * iar);
  ```

### 4. Image Statistics (ImageStatistics.cs)

#### Histogram Generation
- **Line 152**: `Math.Floor((double)Math.Min(maxPossibleValue, x.Index) * factor)`
  - Rounds down for histogram binning

### 5. Contrast Detection (ContrastDetection.cs)

- **Lines 165, 171**: Uses same pattern as StarDetection for min/max star size calculations

### 6. Bayer Filter (BayerFilter16bpp.cs)

- **Line 151**: `LRGBArrays.Lum[counter] = (ushort)Math.Floor((dst[RGB.R] + dst[RGB.G] + dst[RGB.B]) / 3d);`
  - Rounds down when calculating luminance from RGB

### 7. Coordinate Calculations (AstroUtil.cs)

- **Line 417-419**: DMS formatting uses Math.Floor for degrees and arcminutes:
  ```csharp
  var degree = Math.Floor(value);
  var arcmin = Math.Floor(DegreeToArcmin(value - degree));
  var arcsec = Math.Round(DegreeToArcsec(value - degree - arcminDeg), 0);
  ```
- **Line 421**: Arc seconds are rounded to nearest integer using banker's rounding

### 8. Input Coordinates (InputCoordinates.cs)

- Coordinate formatting uses Math.Round with 5 decimal places for seconds
- Integer seconds use (int)Math.Round which truncates after rounding

## Rounding Strategy Summary

1. **HFR Calculation**: Uses banker's rounding (default .NET behavior) for background-subtracted pixel values
2. **Image Dimensions**: Consistently uses Floor to round down, ensuring images don't exceed boundaries
3. **Star Size Limits**: Floor for minimum (conservative), Ceiling for maximum (inclusive)
4. **Blur Operations**: Always Floor for pixel values to avoid overflow
5. **Coordinates**: Mixed approach - Floor for degrees/minutes, Round for seconds

## Implications

The use of banker's rounding (round-to-even) in HFR calculations means:
- Values exactly at 0.5 will alternate between rounding up and down
- This provides statistical fairness over many measurements
- May introduce slight variations in HFR when values are near .5 boundaries

The consistent use of Floor for image dimensions ensures:
- No buffer overruns
- Conservative sizing that stays within array bounds
- Predictable behavior across different image scales

## Recommendations

1. Consider documenting the rounding strategy, especially for HFR calculations
2. The default banker's rounding is generally good for statistical calculations
3. The conservative Floor approach for dimensions is appropriate for safety
4. No issues identified that would cause systematic errors in star detection