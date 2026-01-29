use crate::error::{AppError, AppResult};
use serde::Deserialize;

/// Coverage subset parameters
#[derive(Debug, Default, Deserialize)]
pub struct CoverageSubsetParams {
    /// Subset by axis (e.g., "Lat(40:50),Long(-10:10)")
    pub subset: Option<String>,

    /// Scale factor
    pub scale_factor: Option<f64>,

    /// Scale axes (e.g., "Lat(0.5),Long(0.5)")
    pub scale_axes: Option<String>,

    /// Scale size (e.g., "Lat(256),Long(256)")
    pub scale_size: Option<String>,

    /// Output CRS
    pub crs: Option<String>,

    /// Subset CRS
    pub subset_crs: Option<String>,

    /// Output format
    #[serde(rename = "f")]
    pub format: Option<String>,
}

/// Parsed axis subset
#[derive(Debug, Clone)]
pub struct AxisSubset {
    pub axis: String,
    pub low: Option<f64>,
    pub high: Option<f64>,
}

impl CoverageSubsetParams {
    /// Parse subset parameter into individual axis subsets
    pub fn parse_subset(&self) -> AppResult<Vec<AxisSubset>> {
        let subset = match &self.subset {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };

        let mut result = Vec::new();

        // Parse format: "Axis(low:high),Axis2(low:high)"
        for part in subset.split(',') {
            let part = part.trim();
            if let Some(open_paren) = part.find('(') {
                if let Some(close_paren) = part.find(')') {
                    let axis = part[..open_paren].to_string();
                    let range = &part[open_paren + 1..close_paren];

                    let (low, high) = if range.contains(':') {
                        let parts: Vec<&str> = range.split(':').collect();
                        if parts.len() != 2 {
                            return Err(AppError::BadRequest(format!(
                                "Invalid subset range: {}",
                                range
                            )));
                        }
                        let low = if parts[0].is_empty() {
                            None
                        } else {
                            Some(parts[0].parse().map_err(|_| {
                                AppError::BadRequest(format!("Invalid number: {}", parts[0]))
                            })?)
                        };
                        let high = if parts[1].is_empty() {
                            None
                        } else {
                            Some(parts[1].parse().map_err(|_| {
                                AppError::BadRequest(format!("Invalid number: {}", parts[1]))
                            })?)
                        };
                        (low, high)
                    } else {
                        // Single value - slice at that point
                        let val: f64 = range.parse().map_err(|_| {
                            AppError::BadRequest(format!("Invalid number: {}", range))
                        })?;
                        (Some(val), Some(val))
                    };

                    result.push(AxisSubset { axis, low, high });
                }
            }
        }

        Ok(result)
    }

    /// Get output format with default
    pub fn output_format(&self) -> &str {
        self.format.as_deref().unwrap_or("image/tiff")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_subset() {
        let mut params = CoverageSubsetParams::default();
        params.subset = Some("Lat(40:50),Long(-10:10)".to_string());

        let subsets = params.parse_subset().unwrap();
        assert_eq!(subsets.len(), 2);
        assert_eq!(subsets[0].axis, "Lat");
        assert_eq!(subsets[0].low, Some(40.0));
        assert_eq!(subsets[0].high, Some(50.0));
        assert_eq!(subsets[1].axis, "Long");
        assert_eq!(subsets[1].low, Some(-10.0));
        assert_eq!(subsets[1].high, Some(10.0));
    }

    #[test]
    fn test_parse_open_subset() {
        let mut params = CoverageSubsetParams::default();
        params.subset = Some("Lat(:50),Long(-10:)".to_string());

        let subsets = params.parse_subset().unwrap();
        assert_eq!(subsets[0].low, None);
        assert_eq!(subsets[0].high, Some(50.0));
        assert_eq!(subsets[1].low, Some(-10.0));
        assert_eq!(subsets[1].high, None);
    }
}
