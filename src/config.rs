#[derive(serde::Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Links {
    pub trim_index_html: Option<bool>,
}

#[derive(serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SiteConfig {
    pub base_url: String,
    pub base_url_develop: String,
    #[serde(default)]
    pub links: Links,
}
