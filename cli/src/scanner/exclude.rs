//! Exclude patterns: a small gitignore-like subset, matched on path bytes.
//!
//! - `*` matches any run of characters within one path segment, and `?` one character.
//! - A `**` segment matches zero or more whole segments.
//! - A leading `/` anchors the pattern to the scan root. Otherwise it matches at any
//!   depth, so `node_modules` or `*.log` match an entry with that name anywhere and
//!   `build/cache` matches those two segments anywhere.
//! - A trailing `/` matches directories only.
//!
//! Keep this behavior in sync with `ExcludeRules.swift` in the Swift package.

use std::io;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct Excludes {
    patterns: Vec<Pattern>,
    sources: Vec<String>,
}

#[derive(Clone, Debug)]
struct Pattern {
    segments: Vec<Vec<u8>>,
    /// Unanchored, single-segment patterns compare only the entry name.
    name_only: bool,
    directories_only: bool,
}

impl Excludes {
    /// Compiles patterns, ignoring blank ones. Returns an error for a pattern that can never match.
    pub fn new<I, S>(patterns: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut excludes = Self::default();
        for source in patterns {
            let source = source.into();
            let trimmed = source.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            excludes.patterns.push(Pattern::new(trimmed)?);
            excludes.sources.push(trimmed.to_owned());
        }
        Ok(excludes)
    }

    /// Reads one pattern per line, skipping blank lines and lines that start with `#`.
    pub fn read_lines(path: &Path) -> io::Result<Vec<String>> {
        let text = std::fs::read_to_string(path)?;
        Ok(text
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .collect())
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// The patterns as given, for reports.
    pub fn sources(&self) -> &[String] {
        &self.sources
    }

    /// Whether a pattern that applies to any entry type matches. Check this before
    /// reading metadata, so excluded paths cost nothing and report no errors.
    pub(super) fn matches_any_type(&self, relative: &[u8], name: &[u8]) -> bool {
        self.patterns
            .iter()
            .any(|pattern| !pattern.directories_only && pattern.matches(relative, name))
    }

    /// Whether a directories-only pattern matches a directory.
    pub(super) fn matches_directory(&self, relative: &[u8], name: &[u8]) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.directories_only && pattern.matches(relative, name))
    }
}

impl Pattern {
    fn new(source: &str) -> Result<Self, String> {
        let (body, directories_only) = match source.strip_suffix('/') {
            Some(body) => (body, true),
            None => (source, false),
        };
        let (body, anchored) = match body.strip_prefix('/') {
            Some(body) => (body, true),
            None => (body, false),
        };
        let mut segments: Vec<Vec<u8>> = body
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.as_bytes().to_vec())
            .collect();
        if segments.is_empty() {
            return Err(format!("The exclude pattern {source:?} matches nothing."));
        }
        let name_only = !anchored && segments.len() == 1 && segments[0] != b"**";
        if !anchored && !name_only {
            segments.insert(0, b"**".to_vec());
        }
        Ok(Self {
            segments,
            name_only,
            directories_only,
        })
    }

    fn matches(&self, relative: &[u8], name: &[u8]) -> bool {
        if self.name_only {
            return segment_matches(&self.segments[0], name);
        }
        let path: Vec<&[u8]> = relative.split(|byte| *byte == b'/').collect();
        let pattern: Vec<&[u8]> = self.segments.iter().map(Vec::as_slice).collect();
        segments_match(&pattern, &path)
    }
}

fn segments_match(pattern: &[&[u8]], path: &[&[u8]]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&b"**", rest)) => (0..=path.len()).any(|skip| segments_match(rest, &path[skip..])),
        Some((segment, rest)) => path.split_first().is_some_and(|(name, deeper)| {
            segment_matches(segment, name) && segments_match(rest, deeper)
        }),
    }
}

/// Matches one segment with `*` and `?`, backtracking only to the most recent `*`.
/// A `**` inside a segment, such as `cache**`, behaves like `*`.
fn segment_matches(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        match pattern.get(p) {
            Some(b'*') => {
                star = Some((p, n));
                p += 1;
            }
            Some(b'?') => {
                p += 1;
                n += char_width(name[n]);
            }
            Some(&byte) if byte == name[n] => {
                p += 1;
                n += 1;
            }
            _ => match star {
                Some((star_p, star_n)) => {
                    let resume = star_n + char_width(name[star_n]);
                    star = Some((star_p, resume));
                    p = star_p + 1;
                    n = resume;
                }
                None => return false,
            },
        }
    }
    pattern[p.min(pattern.len())..]
        .iter()
        .all(|byte| *byte == b'*')
}

/// The byte length of the UTF-8 character starting with `lead`, so `?` and `*` step over
/// whole characters. Bytes that aren't valid UTF-8 lead bytes count as one character.
fn char_width(lead: u8) -> usize {
    match lead {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn excluded(patterns: &[&str], relative: &str, directory: bool) -> bool {
        let excludes = Excludes::new(patterns.iter().copied()).expect("valid patterns");
        let name = relative.rsplit('/').next().unwrap_or(relative);
        excludes.matches_any_type(relative.as_bytes(), name.as_bytes())
            || (directory && excludes.matches_directory(relative.as_bytes(), name.as_bytes()))
    }

    /// The case table shared with the Swift `ExcludeRulesTests`.
    #[test]
    fn matches_the_shared_pattern_table() {
        let table: serde_json::Value =
            serde_json::from_str(include_str!("../../../fixtures/exclude-patterns.json"))
                .expect("valid fixture JSON");
        let cases = table["cases"].as_array().expect("cases array");
        assert!(!cases.is_empty());
        for case in cases {
            let patterns: Vec<&str> = case["patterns"]
                .as_array()
                .expect("patterns")
                .iter()
                .map(|pattern| pattern.as_str().expect("string pattern"))
                .collect();
            let relative = case["path"].as_str().expect("path");
            let directory = case["directory"].as_bool().expect("directory");
            assert_eq!(
                excluded(&patterns, relative, directory),
                case["excluded"].as_bool().expect("excluded"),
                "{patterns:?} against {relative:?} (directory: {directory})"
            );
        }
    }

    #[test]
    fn rejects_empty_patterns_and_skips_blank_lines() {
        assert!(Excludes::new(["/"]).is_err());
        assert!(Excludes::new(["//"]).is_err());
        let excludes = Excludes::new(["", "  ", "target  "]).expect("valid");
        assert_eq!(excludes.sources(), ["target"]);
    }

    #[test]
    fn handles_non_utf8_names() {
        let excludes = Excludes::new(["*.bin"]).expect("valid");
        assert!(excludes.matches_any_type(b"a/\xFF\xFE.bin", b"\xFF\xFE.bin"));
    }
}
