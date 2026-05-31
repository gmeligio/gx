//! Helper for declaring compile-once static regexes.

/// Declare a module-level `static` holding a [`regex::Regex`] compiled lazily on first
/// use from a string-literal pattern.
///
/// Compiling a regex is expensive, so a regex used more than once — or used inside a
/// loop — belongs in a `static` rather than being rebuilt at each call. This macro
/// removes the [`std::sync::LazyLock`] boilerplate. The pattern is a string literal
/// known valid at authoring time, so the `expect` cannot fire at runtime; a typo'd
/// pattern is a bug for the owning module's tests to catch, not a condition to handle.
///
/// ```ignore
/// static_regex!(USES_RE, r"^([^@\s]+)@([^\s#]+)");
/// // ... later:
/// if let Some(cap) = USES_RE.captures(text) { /* ... */ }
/// ```
macro_rules! static_regex {
    ($name:ident, $pattern:literal $(,)?) => {
        static $name: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new($pattern).expect(concat!("invalid static regex pattern: ", $pattern))
        });
    };
}

pub(crate) use static_regex;

#[cfg(test)]
#[expect(clippy::expect_used, reason = "tests use expect freely")]
mod tests {
    static_regex!(WORD_RE, r"\b(\w+)\b");

    #[test]
    fn macro_compiles_and_matches() {
        let cap = WORD_RE.captures("hello").expect("should match a word");
        assert_eq!(&cap[1], "hello");
    }
}
