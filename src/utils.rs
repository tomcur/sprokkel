use anyhow::anyhow;
use std::path::Path;

/// Turn a path into a URL with a given prefix. If a scheme and host is given, the path becomes an
/// absolute URL.
pub fn path_to_url(scheme_and_host: Option<&str>, path: impl AsRef<Path>) -> anyhow::Result<String> {
    let path = path.as_ref();

    // allocate roughly enough for the resulting string
    let mut builder = String::with_capacity(
        (scheme_and_host.map(|s| s.len() + 1).unwrap_or(0) + path.into_iter().map(|p| p.len()).sum::<usize>())
            .next_power_of_two(),
    );
    if let Some(s) = scheme_and_host {
        builder.push_str(s);
    }

    for (idx, part) in path.into_iter().enumerate() {
        if idx > 0 || scheme_and_host.is_some() {
            builder.push('/');
        }
        builder.push_str(part.to_str().ok_or(anyhow!("expected UTF-8 path"))?);
    }

    builder.shrink_to_fit();
    Ok(builder)
}

#[cfg(test)]
mod test {
    #[test]
    fn path_to_url() {
        use super::path_to_url;
        use std::path::PathBuf;

        assert_eq!(path_to_url(None, "index.html").unwrap(), "index.html");
        assert_eq!(
            path_to_url(Some("https://example.com"), "index.html").unwrap(),
            "https://example.com/index.html"
        );
        assert_eq!(
            path_to_url(Some("https://example.com"), PathBuf::from("nested").join("file.xml")).unwrap(),
            "https://example.com/nested/file.xml"
        );
    }
}
