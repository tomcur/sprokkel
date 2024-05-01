#[derive(serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SiteConfig {
    pub base_url: String,
    pub base_url_develop: String,
}
