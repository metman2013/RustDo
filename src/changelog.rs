//! The changelog, built into the app from `CHANGELOG.md`, for the "What's new" message.

/// The whole changelog as shipped with this build.
pub const TEXT: &str = include_str!("../CHANGELOG.md");

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub date: String,
    /// ("New", ["Sticky notes…", …]), ("Fixed", […]), …
    pub sections: Vec<(String, Vec<String>)>,
}

/// Parses `## <version> - <date>` headings, `### <section>` headings and `- ` items.
pub fn parse(text: &str) -> Vec<Release> {
    let mut out: Vec<Release> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if let Some(head) = line.strip_prefix("## ") {
            let (version, date) = head.split_once(" - ").unwrap_or((head, ""));
            out.push(Release { version: version.trim().to_string(), date: date.trim().to_string(), sections: Vec::new() });
        } else if let Some(section) = line.strip_prefix("### ") {
            if let Some(r) = out.last_mut() {
                r.sections.push((section.trim().to_string(), Vec::new()));
            }
        } else if let Some(item) = line.strip_prefix("- ") {
            if let Some((_, items)) = out.last_mut().and_then(|r| r.sections.last_mut()) {
                items.push(item.trim().to_string());
            }
        }
    }
    out
}

/// All releases in the built-in changelog, newest first.
pub fn releases() -> Vec<Release> {
    parse(TEXT)
}

/// `0.2.5` → (0, 2, 5). Anything unreadable counts as 0.0.0.
pub fn version_key(v: &str) -> (u32, u32, u32) {
    let mut parts = v.trim().trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

/// What to show after an update: releases newer than `last_seen`, up to `current`, newest
/// first. `None` (an older RustDo that didn't record it) shows just the current release.
pub fn unseen(releases: &[Release], last_seen: Option<&str>, current: &str) -> Vec<Release> {
    let cur = version_key(current);
    match last_seen {
        Some(seen) => {
            let seen = version_key(seen);
            releases
                .iter()
                .filter(|r| {
                    let k = version_key(&r.version);
                    k > seen && k <= cur
                })
                .cloned()
                .collect()
        }
        None => releases.iter().filter(|r| version_key(&r.version) == cur).cloned().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_version_has_a_changelog_entry() {
        let current = env!("CARGO_PKG_VERSION");
        let all = releases();
        let entry = all.iter().find(|r| r.version == current);
        assert!(entry.is_some(), "CHANGELOG.md needs a '## {current} - <date>' section");
        let entry = entry.unwrap();
        assert!(!entry.date.is_empty(), "{current} needs a date");
        assert!(entry.sections.iter().any(|(_, items)| !items.is_empty()), "{current} lists no changes");
        // Newest first, and the current version is the newest.
        assert_eq!(all[0].version, current);
        let keys: Vec<_> = all.iter().map(|r| version_key(&r.version)).collect();
        assert!(keys.windows(2).all(|w| w[0] > w[1]), "versions must be in descending order");
    }

    #[test]
    fn changelog_text_passes_the_windows_build_path_check() {
        // tools/build-release.ps1 rejects an exe containing "/RustDo/" or "\RustDo\"
        // (they look like local paths), and this text is built into the exe.
        let lower = TEXT.to_lowercase();
        assert!(!lower.contains("/rustdo/") && !lower.contains("\\rustdo\\"), "write links without /RustDo/ in CHANGELOG.md");
    }

    #[test]
    fn parses_versions_sections_and_items() {
        let text = "# Changelog\nintro\n\n## 1.1.0 - 2026-01-02\n\n### New\n- A\n- B\n### Fixed\n- C\n\n## 1.0.0 - 2026-01-01\n### New\n- First\n";
        let r = parse(text);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].version, "1.1.0");
        assert_eq!(r[0].date, "2026-01-02");
        assert_eq!(r[0].sections, vec![("New".into(), vec!["A".into(), "B".into()]), ("Fixed".into(), vec!["C".into()])]);
        assert_eq!(r[1].sections[0].1, vec!["First".to_string()]);
    }

    #[test]
    fn shows_everything_since_the_last_version_seen() {
        let text = "## 0.3.0 - d\n### New\n- c\n## 0.2.5 - d\n### New\n- b\n## 0.2.1 - d\n### New\n- a\n## 0.2.0 - d\n### New\n- z\n";
        let all = parse(text);
        let v = |rs: Vec<Release>| rs.into_iter().map(|r| r.version).collect::<Vec<_>>();
        assert_eq!(v(unseen(&all, Some("0.2.1"), "0.2.5")), vec!["0.2.5"]);
        assert_eq!(v(unseen(&all, Some("0.2.0"), "0.3.0")), vec!["0.3.0", "0.2.5", "0.2.1"]);
        assert_eq!(v(unseen(&all, Some("0.2.5"), "0.2.5")), Vec::<String>::new());
        assert_eq!(v(unseen(&all, None, "0.2.5")), vec!["0.2.5"]);
        assert!(version_key("0.10.0") > version_key("0.9.9"));
    }
}
