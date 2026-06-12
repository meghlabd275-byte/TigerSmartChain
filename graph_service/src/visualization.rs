//! Graph Visualization

use super::types::*;

// =============================================================================
// GRAPH VISUALIZATION
// =============================================================================

/// Layout
pub enum Layout {
    Force,
    Dagre,
    Circle,
}

/// Visualizer
pub struct Visualizer {
    layout: Layout,
    width: u32,
    height: u32,
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            layout: Layout::Force,
            width: 800,
            height: 600,
        }
    }

    /// Set layout
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    /// Generate SVG
    pub fn generate_svg(&self, graph: &Graph) -> String {
        let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg">"#);
        svg.push_str(&format!(r#"<rect width="{}" height="{}" fill="white"/>"#, self.width, self.height));
        for node in &graph.nodes {
            svg.push_str(&format!(r#"<circle cx="100" cy="100" r="20" fill="steelblue"/>"#));
        }
        svg.push_str("</svg>");
        svg
    }
}

impl Default for Visualizer {
    fn default() -> Self {
        Self::new()
    }
}