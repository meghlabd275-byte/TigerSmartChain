//! Input sanitization module

use html_escape;

/// Sanitizes user input to prevent XSS and injection attacks
pub struct Sanitizer;

impl Sanitizer {
    /// Sanitize string for HTML output
    pub fn sanitize_html(input: &str) -> String {
        html_escape::encode_text(input).to_string()
    }
    
    /// Sanitize for JavaScript
    pub fn sanitize_js(input: &str) -> String {
        input
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\'', "\\\'")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    }
    
    /// Sanitize for SQL (use parameterized queries instead!)
    pub fn sanitize_sql(_input: &str) -> String {
        // WARNING: This is not safe! Use parameterized queries instead
        String::new()
    }
    
    /// Sanitize for URL
    pub fn sanitize_url(input: &str) -> Option<String> {
        if input.chars().all(|c| c.is_ascii_alphanumeric() || "-_.~".contains(c)) {
            Some(input.to_string())
        } else {
            None
        }
    }
    
    /// Sanitize address for display
    pub fn sanitize_address(address: &str) -> String {
        let addr = address.trim();
        if addr.len() >= 42 {
            format!("{}...{}", &addr[..6], &addr[38..])
        } else {
            addr.to_string()
        }
    }
    
    /// Remove control characters
    pub fn remove_control_chars(input: &str) -> String {
        input.chars().filter(|c| !c.is_control()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_html_sanitization() {
        let input = "<script>alert('xss')</script>";
        let output = Sanitizer::sanitize_html(input);
        assert!(!output.contains("<script>"));
    }
    
    #[test]
    fn test_js_sanitization() {
        let input = "test\"quote";
        let output = Sanitizer::sanitize_js(input);
        assert!(output.contains("\\\""));
    }
}
