use std::collections::HashMap;

use crate::types;

/// Parses front matter from the content string. Returns the front matter and the rest of the
/// content.
pub fn parse_front_matter(content: &str) -> anyhow::Result<(types::FrontMatter, &str)> {
    let mut parsed: Option<HashMap<String, minijinja::Value>> = None;
    let mut rest = content;

    if content.starts_with("+++") {
        if let Some(end) = content[3..].find("\n+++").map(|idx| idx + 3) {
            parsed = Some(toml::from_str(&content[3..end + 1])?);
            rest = &content[end + 4..];
        }
    } else if content.starts_with("---") {
        if let Some(end) = content[3..].find("\n---").map(|idx| idx + 3) {
            parsed = Some(serde_yaml::from_str(&content[3..end + 1])?);
            rest = &content[end + 4..];
        }
    }

    let mut front_matter = types::FrontMatter {
        title: String::new(),
        released: None,
        extra: parsed.unwrap_or_else(|| HashMap::new()),
    };

    let extra = &front_matter.extra;
    if let Some(release) = extra.get("release") {
        front_matter.released =
            Some(release.is_true() || matches!(release.as_str(), Some("true" | "yes")));
    }

    Ok((front_matter, rest))
}
