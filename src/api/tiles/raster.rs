/// Raster tile rendering utilities
use crate::error::{AppError, AppResult};

/// Supported raster tile formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterFormat {
    Png,
    WebP,
    Jpeg,
}

impl RasterFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "png" => Some(RasterFormat::Png),
            "webp" => Some(RasterFormat::WebP),
            "jpg" | "jpeg" => Some(RasterFormat::Jpeg),
            _ => None,
        }
    }

    pub fn content_type(&self) -> &'static str {
        match self {
            RasterFormat::Png => "image/png",
            RasterFormat::WebP => "image/webp",
            RasterFormat::Jpeg => "image/jpeg",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            RasterFormat::Png => "png",
            RasterFormat::WebP => "webp",
            RasterFormat::Jpeg => "jpg",
        }
    }
}

/// Parameters for raster tile rendering
#[derive(Debug, Clone)]
pub struct RasterTileParams {
    pub z: u32,
    pub x: u32,
    pub y: u32,
    pub format: RasterFormat,
    pub tile_size: u32,
}

impl Default for RasterTileParams {
    fn default() -> Self {
        Self {
            z: 0,
            x: 0,
            y: 0,
            format: RasterFormat::Png,
            tile_size: 256,
        }
    }
}

/// Calculate tile bounds in Web Mercator (EPSG:3857)
pub fn tile_bounds_3857(z: u32, x: u32, y: u32) -> (f64, f64, f64, f64) {
    let n = 2_f64.powi(z as i32);
    let tile_size = 2.0 * 20037508.342789244 / n;

    let minx = -20037508.342789244 + (x as f64) * tile_size;
    let maxx = minx + tile_size;
    let maxy = 20037508.342789244 - (y as f64) * tile_size;
    let miny = maxy - tile_size;

    (minx, miny, maxx, maxy)
}

/// Calculate appropriate overview level for a given zoom
/// Returns the overview index (0 = full resolution, 1 = 2x, 2 = 4x, etc.)
pub fn overview_level_for_zoom(z: u32, raster_resolution: f64, tile_size: u32) -> i32 {
    // Calculate the resolution needed for this zoom level
    let earth_circumference = 2.0 * 20037508.342789244;
    let n = 2_f64.powi(z as i32);
    let tile_resolution = earth_circumference / (n * tile_size as f64);

    // Find the overview level that best matches this resolution
    let ratio = tile_resolution / raster_resolution;
    if ratio <= 1.0 {
        return 0; // Full resolution
    }

    (ratio.log2().floor() as i32).max(0)
}

/// Render a raster tile using GDAL
/// This requires the `gdal-support` feature to be enabled
#[cfg(feature = "gdal-support")]
pub fn render_raster_tile_gdal(
    cog_href: &str,
    params: &RasterTileParams,
) -> AppResult<Vec<u8>> {
    use gdal::raster::{RasterBand, ResampleAlg};
    use gdal::Dataset;

    // Convert S3 URL to GDAL VSI path
    let vsi_path = href_to_vsi_path(cog_href);

    // Open the dataset
    let dataset = Dataset::open(&vsi_path)
        .map_err(|e| AppError::Processing(format!("Failed to open raster: {}", e)))?;

    // Get raster dimensions
    let (raster_width, raster_height) = dataset.raster_size();
    let geo_transform = dataset
        .geo_transform()
        .map_err(|e| AppError::Processing(format!("Failed to get geotransform: {}", e)))?;

    // Calculate tile bounds in Web Mercator
    let (tile_minx, tile_miny, tile_maxx, tile_maxy) =
        tile_bounds_3857(params.z, params.x, params.y);

    // Get the raster's projection and bounds
    let raster_minx = geo_transform[0];
    let raster_maxy = geo_transform[3];
    let pixel_width = geo_transform[1];
    let pixel_height = geo_transform[5].abs();
    let raster_maxx = raster_minx + (raster_width as f64 * pixel_width);
    let raster_miny = raster_maxy - (raster_height as f64 * pixel_height);

    // Check if tile intersects with raster
    if tile_maxx < raster_minx
        || tile_minx > raster_maxx
        || tile_maxy < raster_miny
        || tile_miny > raster_maxy
    {
        // Return transparent tile
        return create_transparent_png(params.tile_size);
    }

    // Calculate pixel coordinates for the tile bounds (clamped to raster extent)
    let src_minx = ((tile_minx - raster_minx) / pixel_width).max(0.0) as isize;
    let src_miny = ((raster_maxy - tile_maxy) / pixel_height).max(0.0) as isize;
    let src_maxx = ((tile_maxx - raster_minx) / pixel_width).min(raster_width as f64) as isize;
    let src_maxy = ((raster_maxy - tile_miny) / pixel_height).min(raster_height as f64) as isize;

    let src_width = (src_maxx - src_minx).max(1) as usize;
    let src_height = (src_maxy - src_miny).max(1) as usize;

    // Determine output dimensions (may be smaller if tile extends beyond raster)
    let tile_size = params.tile_size as usize;

    // Calculate destination offset and size for partial tiles
    let dst_offset_x = if tile_minx < raster_minx {
        ((raster_minx - tile_minx) / (tile_maxx - tile_minx) * tile_size as f64) as usize
    } else {
        0
    };
    let dst_offset_y = if tile_maxy > raster_maxy {
        ((tile_maxy - raster_maxy) / (tile_maxy - tile_miny) * tile_size as f64) as usize
    } else {
        0
    };
    let dst_width = (tile_size - dst_offset_x).min(
        ((src_width as f64 * tile_size as f64) / ((tile_maxx - tile_minx) / pixel_width)) as usize,
    );
    let dst_height = (tile_size - dst_offset_y).min(
        ((src_height as f64 * tile_size as f64) / ((tile_maxy - tile_miny) / pixel_height)) as usize,
    );

    // Read bands and create image
    let band_count = dataset.raster_count();
    let mut rgba_buffer = vec![0u8; tile_size * tile_size * 4];

    // Initialize to transparent
    for i in 0..tile_size * tile_size {
        rgba_buffer[i * 4 + 3] = 0; // Alpha = 0
    }

    if band_count >= 3 {
        // RGB or RGBA raster
        for (band_idx, rgba_idx) in [(1, 0), (2, 1), (3, 2)] {
            let band = dataset
                .rasterband(band_idx)
                .map_err(|e| AppError::Processing(format!("Failed to get band: {}", e)))?;

            let data: Vec<u8> = band
                .read_as::<u8>(
                    (src_minx, src_miny),
                    (src_width, src_height),
                    (dst_width, dst_height),
                    Some(ResampleAlg::Bilinear),
                )
                .map_err(|e| AppError::Processing(format!("Failed to read band: {}", e)))?
                .data()
                .to_vec();

            // Copy to output buffer with offset
            for row in 0..dst_height {
                for col in 0..dst_width {
                    let src_idx = row * dst_width + col;
                    let dst_row = row + dst_offset_y;
                    let dst_col = col + dst_offset_x;
                    let dst_idx = (dst_row * tile_size + dst_col) * 4 + rgba_idx;
                    if src_idx < data.len() && dst_idx < rgba_buffer.len() {
                        rgba_buffer[dst_idx] = data[src_idx];
                        // Set alpha to 255 for visible pixels
                        rgba_buffer[(dst_row * tile_size + dst_col) * 4 + 3] = 255;
                    }
                }
            }
        }

        // Handle alpha band if present
        if band_count >= 4 {
            let band = dataset
                .rasterband(4)
                .map_err(|e| AppError::Processing(format!("Failed to get alpha band: {}", e)))?;

            let data: Vec<u8> = band
                .read_as::<u8>(
                    (src_minx, src_miny),
                    (src_width, src_height),
                    (dst_width, dst_height),
                    Some(ResampleAlg::Bilinear),
                )
                .map_err(|e| AppError::Processing(format!("Failed to read alpha band: {}", e)))?
                .data()
                .to_vec();

            for row in 0..dst_height {
                for col in 0..dst_width {
                    let src_idx = row * dst_width + col;
                    let dst_row = row + dst_offset_y;
                    let dst_col = col + dst_offset_x;
                    let dst_idx = (dst_row * tile_size + dst_col) * 4 + 3;
                    if src_idx < data.len() && dst_idx < rgba_buffer.len() {
                        rgba_buffer[dst_idx] = data[src_idx];
                    }
                }
            }
        }
    } else {
        // Single band - render as grayscale
        let band = dataset
            .rasterband(1)
            .map_err(|e| AppError::Processing(format!("Failed to get band: {}", e)))?;

        let data: Vec<u8> = band
            .read_as::<u8>(
                (src_minx, src_miny),
                (src_width, src_height),
                (dst_width, dst_height),
                Some(ResampleAlg::Bilinear),
            )
            .map_err(|e| AppError::Processing(format!("Failed to read band: {}", e)))?
            .data()
            .to_vec();

        for row in 0..dst_height {
            for col in 0..dst_width {
                let src_idx = row * dst_width + col;
                let dst_row = row + dst_offset_y;
                let dst_col = col + dst_offset_x;
                let dst_idx = (dst_row * tile_size + dst_col) * 4;
                if src_idx < data.len() && dst_idx + 3 < rgba_buffer.len() {
                    let val = data[src_idx];
                    rgba_buffer[dst_idx] = val;     // R
                    rgba_buffer[dst_idx + 1] = val; // G
                    rgba_buffer[dst_idx + 2] = val; // B
                    rgba_buffer[dst_idx + 3] = 255; // A
                }
            }
        }
    }

    // Encode to requested format
    encode_image(&rgba_buffer, tile_size, tile_size, params.format)
}

/// Convert an S3 or HTTP URL to a GDAL VSI path
#[cfg(feature = "gdal-support")]
fn href_to_vsi_path(href: &str) -> String {
    if href.starts_with("s3://") {
        // Convert s3://bucket/key to /vsis3/bucket/key
        format!("/vsis3/{}", &href[5..])
    } else if href.starts_with("https://") || href.starts_with("http://") {
        // Use /vsicurl/ for HTTP(S) URLs
        format!("/vsicurl/{}", href)
    } else {
        // Assume it's a local path
        href.to_string()
    }
}

/// Create a transparent RGBA buffer
pub fn create_transparent_buffer(size: usize) -> Vec<u8> {
    vec![0u8; size * size * 4]
}

/// Encode RGBA buffer to the specified format
pub fn encode_image(rgba: &[u8], width: usize, height: usize, format: RasterFormat) -> AppResult<Vec<u8>> {
    match format {
        RasterFormat::Png => encode_png(rgba, width, height),
        RasterFormat::Jpeg => encode_jpeg(rgba, width, height),
        RasterFormat::WebP => {
            // WebP not yet supported, fall back to PNG
            encode_png(rgba, width, height)
        }
    }
}

/// Encode RGBA buffer to PNG
fn encode_png(rgba: &[u8], width: usize, height: usize) -> AppResult<Vec<u8>> {
    use std::io::Cursor;

    let mut png_data = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut png_data), width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);

        let mut writer = encoder
            .write_header()
            .map_err(|e| AppError::Processing(format!("Failed to write PNG header: {}", e)))?;

        writer
            .write_image_data(rgba)
            .map_err(|e| AppError::Processing(format!("Failed to write PNG data: {}", e)))?;
    }

    Ok(png_data)
}

/// Encode RGBA buffer to JPEG
fn encode_jpeg(rgba: &[u8], width: usize, height: usize) -> AppResult<Vec<u8>> {
    use image::codecs::jpeg::JpegEncoder;
    use image::ImageEncoder;
    use std::io::Cursor;

    // Convert RGBA to RGB (JPEG doesn't support alpha)
    // For transparent pixels, use white background
    let mut rgb_data = Vec::with_capacity(width * height * 3);
    for chunk in rgba.chunks(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];

        // Alpha blending with white background
        let alpha = a as f32 / 255.0;
        let blend = |c: u8| -> u8 {
            ((c as f32 * alpha) + (255.0 * (1.0 - alpha))) as u8
        };

        rgb_data.push(blend(r));
        rgb_data.push(blend(g));
        rgb_data.push(blend(b));
    }

    let mut jpeg_data = Vec::new();
    {
        let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut jpeg_data), 85);
        encoder
            .encode(&rgb_data, width as u32, height as u32, image::ExtendedColorType::Rgb8)
            .map_err(|e| AppError::Processing(format!("Failed to encode JPEG: {}", e)))?;
    }

    Ok(jpeg_data)
}

/// Main entry point for raster tile rendering
pub fn render_raster_tile(cog_href: &str, params: &RasterTileParams) -> AppResult<Vec<u8>> {
    #[cfg(feature = "gdal-support")]
    {
        render_raster_tile_gdal(cog_href, params)
    }
    #[cfg(not(feature = "gdal-support"))]
    {
        Err(AppError::Processing(
            "Raster tile rendering requires the 'gdal-support' feature. \
            Build with: cargo build --features gdal-support".to_string(),
        ))
    }
}
