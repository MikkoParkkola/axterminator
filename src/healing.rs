//! Self-healing element location system
//!
//! Implements 7-strategy fallback for robust element location.

use pyo3::prelude::*;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::accessibility::AXUIElementRef;
use crate::element::AXElement;
use crate::error::{AXError, AXResult};

/// Healing strategy enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealStrategy {
    /// Use data-testid attribute (most stable)
    DataTestId,
    /// Use aria-label attribute
    AriaLabel,
    /// Use AX identifier
    Identifier,
    /// Use element title
    Title,
    /// Use XPath-like structural path
    XPath,
    /// Use relative position
    Position,
    /// Use VLM for visual matching (last resort)
    VisualVLM,
}

/// Healing configuration
#[pyclass]
#[derive(Debug, Clone)]
pub struct HealingConfig {
    /// Ordered list of strategies to try
    #[pyo3(get, set)]
    pub strategies: Vec<String>,
    /// Maximum time budget for healing (ms)
    #[pyo3(get, set)]
    pub max_heal_time_ms: u64,
    /// Whether to cache successful heals
    #[pyo3(get, set)]
    pub cache_healed: bool,
}

#[pymethods]
impl HealingConfig {
    #[new]
    #[pyo3(signature = (strategies=None, max_heal_time_ms=100, cache_healed=true))]
    fn new(strategies: Option<Vec<String>>, max_heal_time_ms: u64, cache_healed: bool) -> Self {
        Self {
            strategies: strategies.unwrap_or_else(|| {
                vec![
                    "data_testid".to_string(),
                    "aria_label".to_string(),
                    "identifier".to_string(),
                    "title".to_string(),
                    "xpath".to_string(),
                    "position".to_string(),
                    "visual_vlm".to_string(),
                ]
            }),
            max_heal_time_ms,
            cache_healed,
        }
    }
}

impl Default for HealingConfig {
    fn default() -> Self {
        Self {
            strategies: vec![
                "data_testid".to_string(),
                "aria_label".to_string(),
                "identifier".to_string(),
                "title".to_string(),
                "xpath".to_string(),
                "position".to_string(),
                "visual_vlm".to_string(),
            ],
            max_heal_time_ms: 100,
            cache_healed: true,
        }
    }
}

/// Global healing configuration
static GLOBAL_CONFIG: RwLock<Option<HealingConfig>> = RwLock::new(None);

/// Set the global healing configuration
pub fn set_global_config(config: HealingConfig) -> PyResult<()> {
    let mut global = GLOBAL_CONFIG
        .write()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    *global = Some(config);
    Ok(())
}

/// Get the global healing configuration
pub fn get_global_config() -> HealingConfig {
    GLOBAL_CONFIG
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// Element query for healing
#[derive(Debug, Clone)]
pub struct ElementQuery {
    /// Original query string
    pub original: String,
    /// Original identifier if known
    pub original_id: Option<String>,
    /// Text hint for matching
    pub text_hint: Option<String>,
    /// Structural path for XPath-like matching
    pub path: Option<String>,
    /// Position for relative matching
    pub position: Option<(f64, f64)>,
    /// Screenshot for visual matching
    pub screenshot: Option<Vec<u8>>,
    /// Description for VLM
    pub description: Option<String>,
}

/// Find element with healing
pub fn find_with_healing(query: &ElementQuery, root: AXUIElementRef) -> AXResult<AXElement> {
    let config = get_global_config();
    let start = Instant::now();
    let timeout = Duration::from_millis(config.max_heal_time_ms);

    // Try each strategy in order
    for strategy_name in &config.strategies {
        if start.elapsed() >= timeout {
            break;
        }

        let strategy = parse_strategy(strategy_name);
        if let Some(element) = try_strategy(strategy, query, root) {
            return Ok(element);
        }
    }

    Err(AXError::ElementNotFoundAfterHealing(query.original.clone()))
}

/// Parse strategy name to enum
fn parse_strategy(name: &str) -> HealStrategy {
    match name.to_lowercase().as_str() {
        "data_testid" => HealStrategy::DataTestId,
        "aria_label" => HealStrategy::AriaLabel,
        "identifier" => HealStrategy::Identifier,
        "title" => HealStrategy::Title,
        "xpath" => HealStrategy::XPath,
        "position" => HealStrategy::Position,
        "visual_vlm" => HealStrategy::VisualVLM,
        _ => HealStrategy::Title, // Default fallback
    }
}

/// Try a specific healing strategy
fn try_strategy(
    strategy: HealStrategy,
    query: &ElementQuery,
    root: AXUIElementRef,
) -> Option<AXElement> {
    match strategy {
        HealStrategy::DataTestId => try_by_data_testid(query, root),
        HealStrategy::AriaLabel => try_by_aria_label(query, root),
        HealStrategy::Identifier => try_by_identifier(query, root),
        HealStrategy::Title => try_by_title(query, root),
        HealStrategy::XPath => try_by_xpath(query, root),
        HealStrategy::Position => try_by_position(query, root),
        HealStrategy::VisualVLM => try_by_visual(query, root),
    }
}

fn try_by_data_testid(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // TODO: Implement data-testid search
    None
}

fn try_by_aria_label(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // TODO: Implement aria-label search
    None
}

fn try_by_identifier(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // TODO: Implement identifier search
    None
}

fn try_by_title(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // TODO: Implement title search
    None
}

fn try_by_xpath(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // TODO: Implement structural path search
    None
}

fn try_by_position(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // TODO: Implement position-based search
    None
}

fn try_by_visual(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // TODO: Implement VLM-based visual matching
    // This would use MLX or similar for element identification from screenshots
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HealingConfig::default();
        assert_eq!(config.strategies.len(), 7);
        assert_eq!(config.max_heal_time_ms, 100);
        assert!(config.cache_healed);
    }

    #[test]
    fn test_parse_strategy() {
        assert_eq!(parse_strategy("data_testid"), HealStrategy::DataTestId);
        assert_eq!(parse_strategy("visual_vlm"), HealStrategy::VisualVLM);
    }
}
