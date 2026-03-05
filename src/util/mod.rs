pub mod token_counter;

/// Truncate string with ellipsis at character boundary
pub fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    
    s.chars()
        .take(max_chars)
        .collect::<String>()
        + "..."
}
