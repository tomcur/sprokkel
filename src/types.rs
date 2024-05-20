use anyhow::anyhow;
use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{utils, Ctx};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Date {
    pub fn new(year: u16, month: u8, day: u8) -> Self {
        Date { year, month, day }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Time {
    pub fn new(hour: u8, minute: u8, second: u8) -> Self {
        Time { hour, minute, second }
    }
}

fn parse_date_time(value: &str) -> Result<(Date, Option<Time>), ()> {
    // formats:
    // 2024-04-26
    // or
    // 2024-04-26T123456
    if !(value.len() == 10 || value.len() == 17) || !value.is_char_boundary(10) {
        return Err(());
    }

    let mut date_values = value[..10].split('-');

    let year = date_values.next().ok_or(())?.parse().map_err(|_| ())?;
    let month = date_values.next().ok_or(())?.parse().map_err(|_| ())?;
    let day = date_values.next().ok_or(())?.parse().map_err(|_| ())?;

    let date = Date::new(year, month, day);

    if value.len() == 17 {
        if !matches!(&value[10..11], "T" | "t") {
            return Err(());
        }

        let time: u32 = value[11..].parse().map_err(|_| ())?;
        let hour = time / 1_00_00;
        let minute = (time - hour * 1_00_00) / 1_00;
        let second = time - hour * 1_00_00 - minute * 1_00;

        Ok((date, Some(Time::new(hour as u8, minute as u8, second as u8))))
    } else {
        Ok((date, None))
    }
}

/// Splits a file name on the first underscore. If the part before the underscore is a datetime, it
/// is returned. The slug is the part after the underscore. If there is no underscore, the slug is
/// the entire file name.
fn file_name_into_date_and_slug(file_name: &str) -> (Option<(Date, Option<Time>)>, &str) {
    if let Some(idx) = file_name.find('_') {
        let key = &file_name[..idx];
        let slug = &file_name[idx + 1..];
        (parse_date_time(key).ok(), slug)
    } else {
        (None, file_name)
    }
}

#[derive(Debug)]
pub struct Images {
    pub original: PathBuf,
    pub original_width: Option<u32>,
    pub x_1536: Option<PathBuf>,
    pub x_768: Option<PathBuf>,
}

#[derive(Debug, serde::Serialize)]
pub enum EntrySourceKind {
    Djot,
    CommonMark,
}

#[derive(Debug, serde::Serialize)]
pub struct EntryMeta {
    /// Date is set for entries whose filenames' start with a date in the format `yyyy-mm-dd`
    #[serde(skip)]
    pub sort_key: String,
    pub date: Option<Date>,
    pub time: Option<Time>,
    pub group: String,
    pub slug: String,
    pub source_kind: EntrySourceKind,
    /// e.g., `posts/2024-10-02-foo-bar.dj`, `posts/2024-10-02-foo-bar/index.dj` or
    /// `pages/baz.dj`
    #[serde(skip)]
    pub file_path: PathBuf,
    /// e.g., `posts`, `posts/2024-10-02-foo-bar` or `pages`.
    #[serde(skip)]
    pub asset_dir: PathBuf,
    /// A name by which this entry can be referred to. Consists of the entry group plus the file
    /// name excluding extension. E.g., `posts/2024-10-02-foo-bar`, `posts/2024-10-02-foo-bar` or
    /// `pages/baz`.
    pub canonical_name: String,
    // /// e.g., `/2024/foo-bar.html` or `/baz.html`
    // pub page_url: String,
    // /// e.g., `/2024/foo-bar` or `/baz`.
    // pub asset_url: String,
    /// e.g., `2024/foo-bar.html` or `baz.html`
    #[serde(skip)]
    pub out_file: PathBuf,
    /// e.g., `2024/foo-bar` or `baz`
    #[serde(skip)]
    pub out_asset_dir: PathBuf,
    /// e.g., `2024/foo-bar`
    pub asset_url: String,
    /// e.g., `2024/foo-bar.html`
    pub permalink: String,
}

#[derive(Debug, serde::Serialize)]
pub struct EntryMetaAndFrontMatter<'m> {
    #[serde(flatten)]
    pub meta: &'m EntryMeta,
    #[serde(flatten)]
    pub front_matter: &'m FrontMatter,
}

#[derive(Debug, serde::Serialize)]
pub struct FrontMatter {
    pub title: String,
    pub released: Option<bool>,
    #[serde(rename(serialize = "front_matter"))]
    pub extra: HashMap<String, minijinja::value::Value>,
}

#[derive(Debug, serde::Serialize)]
pub struct Entry<'m> {
    #[serde(flatten)]
    pub meta: &'m EntryMeta,
    #[serde(flatten)]
    pub front_matter: &'m FrontMatter,
    pub summary: String,
    pub rest: String,
}

impl EntryMeta {
    pub fn entry_from_path(ctx: &Ctx, path_prefix: &Path, path: &Path) -> anyhow::Result<Self> {
        let source_kind = match path.extension().map(std::ffi::OsStr::as_encoded_bytes) {
            Some(b"dj") => EntrySourceKind::Djot,
            Some(b"md") => EntrySourceKind::CommonMark,
            _ => anyhow::bail!("Expected entry filename extension to be .dj or .md"),
        };

        let mut path_without_prefix = path.strip_prefix(path_prefix)?.iter();
        let group = path_without_prefix
            .next()
            .ok_or(anyhow!("expected path to have at least two components"))?
            .to_str()
            .ok_or(anyhow!("path is not Unicode"))?
            .to_owned();

        let file_name = {
            let name = path_without_prefix
                .next()
                .ok_or(anyhow!("expected path to have at least two components"))?;

            Path::new(name)
                .file_stem()
                .ok_or(anyhow!("path has no file name"))?
                .to_str()
                .ok_or(anyhow!("expected UTF-8 file name"))?
                .to_owned()
        };

        let parent_dir = path.parent().ok_or(anyhow!("expected file with parent dir"))?;

        // For entries that are in directories, take the directory name. For entries that are
        // directly in the group directory, strip the file suffix.
        let canonical_name = {
            let path = path.strip_prefix(path_prefix).unwrap_or(path);
            let path: Cow<'_, Path> = if path.ends_with("index.dj") {
                Cow::Borrowed(path.parent().unwrap())
            } else {
                Cow::Owned(path.with_extension(""))
            };
            utils::path_to_url(None, path)?
        };

        let (dt, slug) = file_name_into_date_and_slug(&file_name);
        if let Some(dt) = dt {
            let (date, time) = dt;
            let out_file = PathBuf::from(format!("{}", date.year)).join(slug).join("index.html");
            let out_asset_dir = PathBuf::from(format!("{}", date.year)).join(slug);
            Ok(EntryMeta {
                sort_key: file_name.to_owned(),
                group,
                date: Some(date),
                time,
                slug: slug.to_owned(),
                source_kind,
                file_path: path.to_owned(),
                asset_dir: parent_dir.to_owned(),
                canonical_name,
                permalink: ctx.path_to_absolute_url(&out_file).expect("valid path"),
                asset_url: ctx.path_to_absolute_url(&out_asset_dir).expect("valid path"),
                out_file,
                out_asset_dir,
            })
        } else {
            let out_file = if slug == "index" {
                PathBuf::from(format!("{slug}.html"))
            } else {
                PathBuf::from(slug).join("index.html")
            };
            let out_asset_dir = PathBuf::from(slug);
            Ok(EntryMeta {
                sort_key: file_name.to_owned(),
                group,
                date: None,
                time: None,
                slug: slug.to_owned(),
                source_kind,
                file_path: path.to_owned(),
                asset_dir: parent_dir.to_owned(),
                canonical_name,
                permalink: ctx.path_to_absolute_url(&out_file).expect("valid path"),
                asset_url: ctx.path_to_absolute_url(&out_asset_dir).expect("valid path"),
                out_file,
                out_asset_dir,
            })
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn parse_file_name() {
        use super::{file_name_into_date_and_slug, Date};

        assert_eq!(
            file_name_into_date_and_slug("2024-04-16_a-test_!"),
            (Some((Date::new(2024, 04, 16), None)), "a-test_!")
        );
        assert_eq!(
            file_name_into_date_and_slug("2024-04-16-a-test-!"),
            (None, "2024-04-16-a-test-!")
        );
        assert_eq!(file_name_into_date_and_slug("_"), (None, ""));
        assert_eq!(file_name_into_date_and_slug(""), (None, ""));
    }

    #[test]
    fn parse_date_time() {
        use super::{parse_date_time, Date, Time};

        let (date, time) = parse_date_time("2024-04-16").unwrap();
        assert!(time.is_none());
        assert_eq!(
            date,
            Date {
                year: 2024,
                month: 04,
                day: 16,
            }
        );

        let (date, time) = parse_date_time("2024-04-16T094032").unwrap();
        assert_eq!(
            date,
            Date {
                year: 2024,
                month: 04,
                day: 16,
            }
        );
        assert_eq!(
            time,
            Some(Time {
                hour: 9,
                minute: 40,
                second: 32,
            })
        );

        assert!(parse_date_time("2024-04-16T0940320").is_err());
        assert!(parse_date_time("2024-04-16T").is_err());
        assert!(parse_date_time("202-04-16").is_err());
        assert!(parse_date_time("20240416").is_err());
    }
}
