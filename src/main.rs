use anyhow::Context;
use clap::Parser;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

use notify_debouncer_full::{new_debouncer, notify::*, DebounceEventResult};

mod cli;
mod config;
mod djot;
mod front_matter;
mod highlight;
mod images;
mod out;
mod render;
mod types;
mod utils;

use out::Out;

use crate::utils::path_to_http_url;

#[derive(Debug)]
struct Group {
    name: String,
    range: std::ops::Range<usize>,
}

impl Group {
    fn new(name: String, range: std::ops::Range<usize>) -> Self {
        Group { name, range }
    }

    fn remove_idx(&mut self, removed_idx: usize) {
        if removed_idx < self.range.start {
            self.range.start -= 1;
            self.range.end -= 1;
        } else if self.range.contains(&removed_idx) {
            self.range.end -= 1;
        }
    }
}

fn collect_entries<'a>(
    path_prefix: &'a Path,
    path: &Path,
) -> impl Iterator<Item = anyhow::Result<types::EntryMeta>> + 'a {
    walkdir::WalkDir::new(path)
        .follow_links(true)
        .max_depth(2)
        .sort_by_file_name()
        .into_iter()
        .filter_map(move |entry| match entry {
            Ok(entry) => {
                let name = entry.path().to_str()?;
                if entry.file_type().is_dir() || !name.ends_with(".dj") {
                    None
                } else {
                    Some(types::EntryMeta::entry_from_path(path_prefix, entry.path()))
                }
            }
            Err(err) => Some(Err(err.into())),
        })
}

fn collect_entry_groups(
    path: impl AsRef<Path>,
) -> anyhow::Result<(Vec<Group>, Vec<types::EntryMeta>)> {
    let path = path.as_ref();
    let mut entries = vec![];
    let mut groups = vec![];

    for group in walkdir::WalkDir::new(path)
        // skip self
        .min_depth(1)
        .max_depth(1)
        .follow_links(true)
    {
        let group = group?;
        let walk_path = group.path();
        let group_name = walk_path
            .file_name()
            .ok_or(anyhow::anyhow!("entry group has no name"))
            .and_then(|name| {
                name.to_str().ok_or(anyhow::anyhow!(
                    "group name is not representable as Unicode, at: {:?}",
                    walk_path
                ))
            })?;

        if group.file_type().is_dir() {
            let start_idx = entries.len();
            for entry in collect_entries(path, walk_path) {
                entries.push(entry?);
            }
            let end_idx = entries.len();
            groups.push(Group::new(group_name.to_owned(), start_idx..end_idx));
        }
    }

    anyhow::Ok((groups, entries))
}

fn build(
    path: &Path,
    build_kind: cli::BuildKind,
    renderer: &render::Renderer,
    base_url: &str,
) -> anyhow::Result<()> {
    let out = Out::at("./out")?;

    let (groups, entries) = collect_entry_groups(path.join("entries"))?;

    log::info!("Found {} entry group(s):", groups.len());
    for group in groups.iter() {
        log::info!(
            "  entries in group \"{}\": {}",
            group.name,
            group.range.len()
        );
    }

    // This reads all entries' file contents into memory. Fine for now.
    //
    // Bounded memory usage w.r.t. entry file contents would be preferable. A way to do that:
    //
    // 1. Copy/process all non-markup files from entry directories to out directories. Just like
    //    now, keep track of new image transcodes. At the moment a full parsing step is required
    //    because only images actually linked in the markup are included, but that may be an
    //    unnecessary complication.
    // 2. For every entry, parse its front matter (i.e., parse front matter TOML/YAML document +
    //    parse the start of the markup to render the entry title).
    // 3. Then iterate through entries, reading and parsing full file contents from fs and
    //    rendering HTML to fs.
    let content: Vec<String> = entries
        .iter()
        .map(|entry| std::fs::read_to_string(&entry.file_path))
        .collect::<std::io::Result<_>>()?;

    // Parse entry front matter
    let (content, mut front_matter): (Vec<&str>, Vec<types::FrontMatter>) = {
        let mut content: Vec<&str> = content.iter().map(String::as_str).collect();

        let front_matter: Vec<_> = entries
            .par_iter()
            .zip(&mut content)
            .map(
                |(_meta, content)| match front_matter::parse_front_matter(content) {
                    Ok((front_matter, rest)) => {
                        *content = rest;
                        Ok(front_matter)
                    }
                    Err(err) => Err(err),
                },
            )
            .collect::<anyhow::Result<_>>()?;

        (content, front_matter)
    };

    let mut parsed: Vec<Vec<jotdown::Event<'_>>> = content
        .par_iter()
        .map(|content| jotdown::Parser::new(content).collect())
        .collect();

    // Parse entry front matter, consuming the front matter events from `parsed`
    entries
        .iter()
        .zip(&mut parsed)
        .zip(&mut front_matter)
        .for_each(|((meta, parsed), front_matter)| {
            if let Ok(Some(title)) = djot::parse_and_render_title(parsed) {
                front_matter.title = title;
            } else {
                front_matter.title = meta.slug.clone();
            }
        });

    // When in production-mode, filter out non-released entries
    let (groups, entries, mut parsed, front_matter) = if build_kind.is_production() {
        let before = entries.len();

        let mut groups = groups;
        let mut entries = entries;
        let mut parsed = parsed;
        let mut front_matter = front_matter;

        for idx in (0..entries.len()).rev() {
            // if front_matter[idx].released.unwrap_or(false) {
            if front_matter[idx].released {
                continue;
            }

            entries.remove(idx);
            parsed.remove(idx);
            front_matter.remove(idx);

            for group in groups.iter_mut() {
                group.remove_idx(idx);
            }
        }

        let after = entries.len();
        if before != after {
            log::info!("Filtered out {} non-released entries", before - after);
        }

        (groups, entries, parsed, front_matter)
    } else {
        (groups, entries, parsed, front_matter)
    };

    let entries_and_front_matter: Vec<types::EntryMetaAndFrontMatter> = entries
        .iter()
        .zip(&front_matter)
        .map(|(meta, front_matter)| types::EntryMetaAndFrontMatter { meta, front_matter })
        .collect();

    // Rewrite internal links and turn them into "back-references" (as in, for each entry, "which
    // entries link here")
    // Records entry indices: linker => linkee
    let references: Vec<(usize, usize)> = {
        // let mut references: Vec<(usize, usize)> = (0..entries.len()).map(|_| vec![]).collect();
        // let mut references: Vec<(usize, usize)> = Vec::new();

        let entries_by_name: HashMap<&str, &types::EntryMetaAndFrontMatter> = {
            let mut map = HashMap::new();
            for entry in entries_and_front_matter.iter() {
                if map.insert(&*entry.meta.canonical_name, entry).is_some() {
                    anyhow::bail!("Entry name is duplicated: {}", entry.meta.canonical_name);
                }
            }
            map
        };

        // TODO: it would be nice to error on dead anchor links (headings), but collecting anchors
        // requires a full pass of the input files. As links to anchors don't require any link
        // rewriting, perhaps the HTML render step can output entry anchors as a side effect, and
        // sprokkel then checks whether the links are valid.
        let entries_addr = (&entries_and_front_matter[0] as *const _) as usize;
        let references = parsed
            .par_iter_mut()
            .enumerate()
            .map(|(linker_idx, parsed)| {
                let internal_links =
                    djot::rewrite_and_emit_internal_links(parsed, &entries_by_name, base_url)?;

                let mut linkee_indices = internal_links
                    .into_iter()
                    .map(|linkee_entry| {
                        let linkee_addr = (linkee_entry as *const _) as usize;
                        // calculate index of referenced entry by memory address
                        let linkee_idx = (linkee_addr - entries_addr)
                            / std::mem::size_of::<types::EntryMetaAndFrontMatter>();
                        (linker_idx, linkee_idx)
                    })
                    .collect::<Vec<_>>();
                linkee_indices.sort();
                linkee_indices.dedup();

                anyhow::Ok(linkee_indices)
            })
            .try_reduce_with(|mut left, right| {
                left.extend(right);
                anyhow::Ok(left)
            });
        references.unwrap_or(anyhow::Ok(vec![]))?
    };

    let images = images::extract_images(&out, &entries, &parsed)?;

    // Render entry markup to HTML
    let rendered: Vec<_> = entries
        .par_iter()
        .zip(parsed)
        .zip(images)
        .zip(&front_matter)
        .map(|(((meta, parsed), images), front_matter)| {
            djot::render(parsed, &images).map(|(summary, rest)| types::Entry {
                meta,
                front_matter,
                summary,
                rest,
            })
        })
        .collect::<anyhow::Result<_>>()?;

    // Turn the linker => linkee entry indices into a list of &Entry back-references for every
    // entry.
    let references = {
        let mut references_: Vec<Vec<&types::Entry>> = (0..entries.len()).map(|_| vec![]).collect();

        for (linker, linkee) in references {
            references_[linkee].push(&rendered[linker]);
        }

        references_
    };

    let grouped_entries: HashMap<&str, &[types::Entry<'_>]> = groups
        .iter()
        .map(|Group { name, range }| (name.as_str(), &rendered[range.clone()]))
        .collect();
    let render_context = renderer.render_context(&grouped_entries);

    // Render entries to HTML files using the template renderer, streaming results back to be
    // written to out.
    {
        let rendered = &rendered;
        rayon::scope(|s| {
            let (result_tx, result_rx) = mpsc::sync_channel::<(&'_ Path, anyhow::Result<Vec<u8>>)>(
                rayon::current_num_threads(),
            );

            s.spawn(move |s| {
                for (entry, references) in rendered.iter().zip(references) {
                    let result_tx = result_tx.clone();
                    s.spawn(move |_| {
                        let mut write = Vec::new();
                        let res = render_context.entry(&mut write, entry, &references);
                        let _ = result_tx.send((&entry.meta.out_file, res.map(|_| write)));
                    });
                }
            });

            while let Ok((path, result)) = result_rx.recv() {
                let result = result?;
                out.update_file(&mut &*result, path)?;
            }

            anyhow::Ok(())
        })?;
    }

    // Render all template files where no part of the template file path starts with an underscore.
    {
        let path = path.join("templates");
        rayon::scope(|s| -> anyhow::Result<()> {
            let (result_tx, result_rx) = mpsc::sync_channel::<anyhow::Result<(PathBuf, String)>>(
                rayon::current_num_threads(),
            );

            for template_path in walkdir::WalkDir::new(&path).follow_links(true) {
                let template_path = template_path?;
                if template_path.file_type().is_file() {
                    let template_path = template_path.path().strip_prefix(&path)?.to_owned();
                    if !template_path
                        .iter()
                        .any(|p| p.to_string_lossy().chars().nth(0) == Some('_'))
                    {
                        let extension = match template_path.extension() {
                            Some(extension) => Some(
                                extension
                                    .to_str()
                                    .ok_or(anyhow::anyhow!("tempalte file name is not utf-8"))?,
                            ),
                            None => None,
                        };
                        let file_name = template_path
                            .file_stem()
                            .and_then(|name| name.to_str())
                            .ok_or(anyhow::anyhow!("template file name is not utf-8"))?
                            .to_owned();

                        let out_file = {
                            let template_path = template_path.clone();
                            let extension = extension
                                .map(|ext| format!(".{ext}"))
                                .unwrap_or(String::new());
                            move |page| -> PathBuf {
                                if page == 0 {
                                    template_path.with_file_name(format!("{file_name}{extension}"))
                                } else {
                                    let page = page + 1;
                                    template_path
                                        .with_file_name(format!("{file_name}-{page}{extension}"))
                                }
                            }
                        };
                        let page_permalink = {
                            let out_file = out_file.clone();
                            move |page| -> String {
                                let path = out_file(page);
                                path_to_http_url(path).unwrap()
                            }
                        };

                        let result_tx = result_tx.clone();
                        s.spawn(move |_| {
                            let result = render_context.template(template_path, page_permalink);
                            if let Err(err) = result {
                                let _ = result_tx.send(Err(err));
                                return;
                            }
                            let result = result.unwrap();
                            for page in result {
                                let _ = result_tx
                                    .send(page.map(|(page, content)| (out_file(page), content)));
                            }
                        });
                    }
                }
            }
            drop(result_tx);

            while let Ok(result) = result_rx.recv() {
                let (path, content) = result?;
                out.update_file(&mut &*content.as_bytes(), path)?;
            }

            Ok(())
        })?;
    }

    {
        let asset_dir = path.join("assets");
        if asset_dir.exists() {
            out.copy_dir(&asset_dir, ".")?;
        }
    }

    // For every directory in ./cat, concatenate all files
    {
        let path = path.join("cat");
        if path.exists() {
            for dir in walkdir::WalkDir::new(&path).min_depth(1).follow_links(true) {
                let dir = dir?;
                if !dir.file_type().is_dir() {
                    continue;
                }

                let name = dir.path().strip_prefix(&path)?;
                out.cat_dir(dir.path(), name)?;
            }
        }
    }

    Ok(())
}

enum FsChange {
    Template,
    Other,
    None,
}

fn main() -> anyhow::Result<()> {
    {
        use simplelog as s;
        s::TermLogger::init(
            s::LevelFilter::Debug,
            s::Config::default(),
            s::TerminalMode::Mixed,
            s::ColorChoice::Auto,
        )
        .unwrap();
    }

    let args = cli::Args::parse();

    #[allow(irrefutable_let_patterns)]
    let args = if let cli::Commands::Build(args) = args.command {
        args
    } else {
        unimplemented!();
    };
    let build_kind = if args.develop {
        cli::BuildKind::Develop
    } else {
        cli::BuildKind::Production
    };

    let site_config_path = args.path.join("sprokkel.toml");

    if args.watch {
        let cvar_pair = Arc::new((Mutex::new(FsChange::Template), Condvar::new()));
        let cvar_pair2 = cvar_pair.clone();
        let path_prefix = args.path.canonicalize()?;
        let mut debouncer = new_debouncer(
            Duration::from_millis(250),
            None,
            move |ev: DebounceEventResult| {
                let (lock, cvar) = &*cvar_pair2;
                let mut change_ = FsChange::Other;

                if let Ok(evs) = ev {
                    if evs
                        .into_iter()
                        .flat_map(|e| e.event.paths.into_iter())
                        .any(|path| {
                            path.strip_prefix(&path_prefix)
                                .map(|path| path.starts_with("templates"))
                                .unwrap_or(false)
                        })
                    {
                        change_ = FsChange::Template;
                    }
                }

                let mut change = lock.lock().unwrap();
                *change = change_;
                cvar.notify_one();
            },
        )
        .unwrap();

        debouncer
            .watcher()
            .watch(&args.path, RecursiveMode::Recursive)
            .unwrap();
        debouncer
            .cache()
            .add_root(&args.path, RecursiveMode::Recursive);

        let mut site_config: Option<config::SiteConfig> = None;
        let mut renderer: Option<render::Renderer> = None;

        let mut build_watch = move |change: FsChange| -> anyhow::Result<()> {
            let config_changed = {
                let site_config_: config::SiteConfig =
                    toml::from_str(&std::fs::read_to_string(&site_config_path)?)
                        .with_context(|| "Parsing sprokkel.toml")?;

                let config_changed = Some(&site_config_) != site_config.as_ref();
                if config_changed && site_config.is_some() {
                    log::info!("Reloaded sprokkel.toml.");
                }
                site_config = Some(site_config_);
                config_changed
            };
            let site_config = site_config.as_ref().unwrap();

            let base_url = if build_kind.is_production() {
                &site_config.base_url
            } else {
                &site_config.base_url_develop
            };

            if config_changed || matches!(change, FsChange::Template) {
                log::info!("Reloading templates…");
                renderer = Some(render::Renderer::build(
                    base_url.to_owned(),
                    args.path.join("templates"),
                )?);
            }

            log::info!("Building…");
            let instant = std::time::Instant::now();
            if let Err(err) = build(&args.path, build_kind, renderer.as_ref().unwrap(), base_url) {
                log::error!("{:?}", err);
            }
            log::info!(
                "======== Building took {}ms ========",
                std::time::Instant::now()
                    .duration_since(instant)
                    .as_millis()
            );

            Ok(())
        };

        loop {
            let (lock, cvar) = &*cvar_pair;
            let mut change = lock.lock().unwrap();
            while matches!(&*change, &FsChange::None) {
                log::info!("Waiting for file change…");
                change = cvar.wait(change).unwrap();
            }
            let change_ = std::mem::replace(&mut *change, FsChange::None);
            drop(change);

            if let Err(err) = build_watch(change_) {
                log::error!("{:?}", err);
            }
        }
    } else {
        let site_config: config::SiteConfig =
            toml::from_str(&std::fs::read_to_string(&site_config_path)?)?;
        let base_url = if build_kind.is_production() {
            &site_config.base_url
        } else {
            &site_config.base_url_develop
        };
        let renderer = render::Renderer::build(base_url.to_owned(), args.path.join("templates"))?;
        build(&args.path, build_kind, &renderer, base_url)?;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    #[test]
    fn filter_groups() {
        use super::Group;

        let mut group = Group::new("".to_owned(), 0..0);
        group.remove_idx(0);
        assert_eq!(group.range, 0..0);
        group.remove_idx(1);
        assert_eq!(group.range, 0..0);

        let mut group = Group::new("".to_owned(), 0..2);
        group.remove_idx(1);
        assert_eq!(group.range, 0..1);
        group.remove_idx(1);
        assert_eq!(group.range, 0..1);
        group.remove_idx(0);
        assert_eq!(group.range, 0..0);

        let mut group = Group::new("".to_owned(), 1..2);
        group.remove_idx(2);
        assert_eq!(group.range, 1..2);
        group.remove_idx(0);
        assert_eq!(group.range, 0..1);
    }
}
