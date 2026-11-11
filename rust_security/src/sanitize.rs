//! Input sanitization module
//!
//! Escaping is applied at the point of use: HTML for HTML contexts, JavaScript
//! string escaping for JS contexts, and so on.
//!
//! There is deliberately no SQL sanitizer. Escaping a string and pasting it
//! into a query is the root cause of SQL injection; the fix is a bound
//! parameter, which keeps data and code separate. See
//! [`Sanitizer::sql_parameter_guidance`].

use html_escape;

/// Sanitizes user input to prevent XSS and injection attacks
pub struct Sanitizer;

impl Sanitizer {
    /// Sanitize string for HTML output
    pub fn sanitize_html(input: &str) -> String {
        html_escape::encode_text(input).to_string()
    }

    /// Sanitize for JavaScript.
    ///
    /// Produces a value safe to embed inside a JavaScript *string literal*.
    /// It does not make a value safe to interpolate into executable script
    /// source, which no escaping can do.
    pub fn sanitize_js(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        for c in input.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\'' => out.push_str("\\'"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                // U+2028 and U+2029 terminate a line in JavaScript but are not
                // valid inside a string literal.
                '\u{2028}' => out.push_str("\\u2028"),
                '\u{2029}' => out.push_str("\\u2029"),
                // Escape the closing tag sequence so the value cannot break out
                // of a surrounding <script> element.
                '<' => out.push_str("\\u003C"),
                '>' => out.push_str("\\u003E"),
                '&' => out.push_str("\\u0026"),
                c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
                c => out.push(c),
            }
        }
        out
    }

    /// Why there is no SQL escaping helper.
    ///
    /// Any string escaping used to build a query is bypassed by encodings the
    /// escaper did not anticipate. Pass user data as a bound parameter instead:
    ///
    /// ```text
    /// -- wrong: injection is possible
    /// SELECT * FROM t WHERE a = '<escaped input>'
    /// -- right: the driver binds the value, so it cannot become SQL
    /// SELECT * FROM t WHERE a = $1
    /// ```
    pub fn sql_parameter_guidance() -> &'static str {
        "Use bound parameters (e.g. $1) instead of escaping values into SQL text."
    }

    /// Percent-encode a value for safe use in a URL query component.
    ///
    /// Unreserved characters (RFC 3986 §2.3) pass through unchanged; everything
    /// else is percent-encoded as UTF-8 bytes.
    pub fn sanitize_url(input: &str) -> Option<String> {
        let mut out = String::with_capacity(input.len());
        for byte in input.bytes() {
            let c = byte as char;
            if c.is_ascii_alphanumeric() || "-_.~".contains(c) {
                out.push(c);
            } else {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
        Some(out)
    }

    /// Shorten an address for display, e.g. `0x1234...abcd`.
    pub fn sanitize_address(address: &str) -> String {
        let addr = address.trim();
        // 42 characters is a full address; anything shorter is shown as-is.
        if addr.len() >= 42 && addr.is_char_boundary(6) && addr.is_char_boundary(38) {
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
        assert!(output.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_js_sanitization_escapes_quotes_and_tags() {
        let output = Sanitizer::sanitize_js("test\"quote'<script>");
        assert!(output.contains("\\\""));
        assert!(output.contains("\\'"));
        // The angle brackets cannot terminate a <script> element.
        assert!(!output.contains('<'));
        assert!(!output.contains('>'));
    }

    #[test]
    fn test_js_sanitization_escapes_line_separators() {
        let output = Sanitizer::sanitize_js("a\u{2028}b\u{2029}c");
        assert_eq!(output, "a\\u2028b\\u2029c");
    }

    #[test]
    fn test_url_encoding() {
        assert_eq!(Sanitizer::sanitize_url("a b/c?d=e").unwrap(), "a%20b%2Fc%3Fd%3De");
        // Unreserved characters survive.
        assert_eq!(Sanitizer::sanitize_url("safe-_.~AZ09").unwrap(), "safe-_.~AZ09");
    }

    #[test]
    fn test_sanitize_address() {
        let addr = "0x742d35Cc6634C0532925a3b844Bc9e7595f0eB1E";
        assert_eq!(Sanitizer::sanitize_address(addr), "0x742d...eB1E");
        assert_eq!(Sanitizer::sanitize_address("short"), "short");
    }

    #[test]
    fn test_remove_control_chars() {
        assert_eq!(Sanitizer::remove_control_chars("a\u{0}b\nc"), "abc");
    }
}
