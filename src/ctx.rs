use std::{path::Path, sync::Arc};

use crate::{cli::BuildKind, config::SiteConfig, utils};

struct InnerCtx {
    build_kind: BuildKind,
    base_url: String,
    trim_index_html: bool,
}

/// Site build context. The context is cheap to clone.
#[derive(Clone)]
pub struct Ctx {
    inner: Arc<InnerCtx>,
}

impl Ctx {
    pub fn from_site_config(build_kind: BuildKind, site_config: &SiteConfig) -> Self {
        let base_url = if build_kind.is_production() {
            &site_config.base_url
        } else {
            &site_config.base_url_develop
        };
        Ctx {
            inner: Arc::new(InnerCtx {
                build_kind,
                base_url: base_url.clone(),
                trim_index_html: site_config.links.trim_index_html.unwrap_or(true),
            }),
        }
    }

    pub fn build_kind(&self) -> BuildKind {
        self.inner.build_kind
    }

    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    /// Turn a path relative to the output directory into an absolute URL.
    pub fn path_to_absolute_url(&self, path: impl AsRef<Path>) -> anyhow::Result<String> {
        let mut url = utils::path_to_url(Some(self.base_url()), path)?;
        if self.inner.trim_index_html && url.ends_with("/index.html") {
            url.truncate(url.len() - "/index.html".len());
        }
        Ok(url)
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn path_to_absolute_url() {
        use super::{BuildKind, Ctx, SiteConfig};
        use std::path::PathBuf;

        let site_config: SiteConfig = toml::from_str(
            r#"
                base-url = "http://localhost:8080"
                base-url-develop = ".."

                [links]
                trim-index-html = true
            "#,
        )
        .unwrap();
        let ctx = Ctx::from_site_config(BuildKind::Production, &site_config);

        assert_eq!(
            ctx.path_to_absolute_url("").unwrap(),
            "http://localhost:8080"
        );
        assert_eq!(
            ctx.path_to_absolute_url("index.html").unwrap(),
            "http://localhost:8080"
        );
        assert_eq!(
            ctx.path_to_absolute_url(PathBuf::from("a").join("nested").join("file.xml"))
                .unwrap(),
            "http://localhost:8080/a/nested/file.xml"
        );
        assert_eq!(
            ctx.path_to_absolute_url(PathBuf::from("a").join("nested").join("index.html"))
                .unwrap(),
            "http://localhost:8080/a/nested"
        );
        assert_eq!(
            ctx.path_to_absolute_url("no-extension").unwrap(),
            "http://localhost:8080/no-extension"
        );
    }
}
