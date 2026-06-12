//! Widgets Types

use serde::{Deserialize, Serialize};

// =============================================================================
// WIDGETS
// =============================================================================

/// Widget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    pub widget_type: String,
    pub title: String,
    pub config: std::collections::HashMap<String, String>,
}

/// Chart Widget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartWidget {
    pub id: String,
    pub chart_type: String,
    pub data: Vec<DataPoint>,
    pub options: std::collections::HashMap<String, String>,
}

/// Data Point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
}

/// Table Widget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableWidget {
    pub id: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<String>>,
}

/// Column
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub r#type: String,
}

/// Widget Library
pub struct Library {
    widgets: std::collections::HashMap<String, Widget>,
}

impl Library {
    pub fn new() -> Self {
        Self {
            widgets: std::collections::HashMap::new(),
        }
    }

    /// Add widget
    pub fn add(&mut self, widget: Widget) {
        self.widgets.insert(widget.id.clone(), widget);
    }

    /// Get widget
    pub fn get(&self, id: &str) -> Option<&Widget> {
        self.widgets.get(id)
    }
}

impl Default for Library {
    fn default() -> Self {
        Self::new()
    }
}