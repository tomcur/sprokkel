//! here we do some iterator-based node rewriting to output from jotdown's Djot representation to
//! html. It would be much cleaner to rewrite an AST, but at least this way should be relatively
//! performant.

use jotdown::{Attributes, Container, Event, Render};
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    convert::identity,
};

use crate::{highlight, types, utils};

/// Rewrites internal links in the format `~/<canonical name>` (e.g. `posts/2024-04-23-something`)
/// to the HTTP URL. Returns the entries this entry links to.
pub fn rewrite_and_emit_internal_links<'entries>(
    events: &mut Vec<Event<'_>>,
    entries_by_name: &HashMap<&str, &'entries types::EntryMetaAndFrontMatter<'entries>>,
    root_url: &str,
) -> anyhow::Result<Vec<&'entries types::EntryMetaAndFrontMatter<'entries>>> {
    let mut internal_links = vec![];

    fn rewrite_link<'entries>(
        old_link: &mut Cow<'_, str>,
        entries_by_name: &HashMap<&str, &'entries types::EntryMetaAndFrontMatter<'entries>>,
        root_url: &str,
    ) -> anyhow::Result<Option<&'entries types::EntryMetaAndFrontMatter<'entries>>> {
        if &old_link[0..2] == "~/" {
            let (link, anchor) = match old_link.find('#') {
                Some(anchor_idx) => (&old_link[2..anchor_idx], &old_link[anchor_idx..]),
                None => (&old_link[2..], ""),
            };

            if let Some(entry) = entries_by_name.get(link) {
                *old_link = Cow::Owned(format!("{root_url}/{}{}", &entry.meta.permalink, anchor));
                return Ok(Some(entry));
            } else {
                anyhow::bail!("Unknown internal link: {old_link}");
            }
        }

        Ok(None)
    }

    for event in events {
        match event {
            Event::Start(Container::Link(link, _), _) => {
                if let Some(entry) = rewrite_link(link, entries_by_name, root_url)? {
                    internal_links.push(entry);
                }
            }
            Event::End(Container::Link(link, _)) => {
                rewrite_link(link, entries_by_name, root_url)?;
            }
            _ => {}
        }
    }

    Ok(internal_links)
}

/// HTML-attribute encode a string in one forward pass, copying the string only if characters that
/// need to be replaced occur.
fn html_attr_encode(val: &str) -> Cow<'_, str> {
    fn mapping(c: char) -> Option<&'static str> {
        match c {
            '<' => Some("&lt;"),
            '>' => Some("&gt;"),
            '&' => Some("&amp;"),
            '"' => Some("&quot;"),
            '\'' => Some("&#x27;"),
            _ => None,
        }
    }

    let mut cow = Cow::Borrowed(val);
    let mut chars = val.chars();
    {
        let mut chars = (&mut chars).enumerate();
        while let Some((idx, c)) = chars.next() {
            if let Some(mapping) = mapping(c) {
                let mut builder = String::with_capacity((val.len() + 1).next_power_of_two());
                builder.push_str(&val[..idx]);
                builder.push_str(mapping);

                cow = Cow::Owned(builder);
                break;
            }
        }
    }

    if let Cow::Owned(ref mut builder) = cow {
        for c in chars {
            if let Some(mapping) = mapping(c) {
                builder.push_str(mapping);
            } else {
                builder.push(c);
            }
        }

        builder.shrink_to_fit();
    }

    cow
}

enum TagCloseKind {
    Open,
    Closed,
}

fn format_open_tag<'a>(
    tag: &str,
    attributes: impl Iterator<Item = (&'a str, impl AsRef<str>)>,
    close_kind: TagCloseKind,
) -> String {
    use std::fmt::Write;

    let mut builder = String::new();

    write!(builder, "<{tag}").unwrap();

    for (attr, value) in attributes {
        let encoded_value = html_attr_encode(value.as_ref());
        write!(builder, r#" {attr}="{encoded_value}""#).unwrap();
    }

    if matches!(close_kind, TagCloseKind::Closed) {
        write!(builder, " /").unwrap();
    }
    write!(builder, ">").unwrap();

    builder
}

/// Iterates from an Event::Start to a matching Event::End. The resulting iterator yields all
/// events in between the start and end, skipping over the start and end itself. If the next item
/// in the iterator is not a Event::Start, the resulting iterator is immediately empty.
fn iter_container<'a>(iter: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    let mut depth = 0usize;
    iter.take_while(move |event| match event {
        Event::Start(_, _) => {
            depth += 1;
            true
        }
        Event::End(_) => {
            depth -= 1;
            depth > 0
        }
        _ => true,
    })
    .skip(1)
}

/// Same as `iter_container` but assumes it Event::Start has already been consumed.
fn iter_container_from_inside<'a>(
    iter: impl Iterator<Item = Event<'a>>,
) -> impl Iterator<Item = Event<'a>> {
    iter_container(
        // prepend a fake start block
        [Event::Start(Container::Paragraph, Attributes::new())]
            .into_iter()
            .chain(iter),
    )
}

fn write_raw_html_inline<'a>(
    html: impl Into<Cow<'a, str>>,
    attr: Option<Attributes<'a>>,
) -> [Event<'a>; 3] {
    [
        Event::Start(
            Container::RawInline { format: "html" },
            attr.unwrap_or(Attributes::new()),
        ),
        Event::Str(html.into()),
        Event::End(Container::RawInline { format: "html" }),
    ]
}

fn write_raw_html_block<'a>(
    html: impl Into<Cow<'a, str>>,
    attr: Option<Attributes<'a>>,
) -> [Event<'a>; 3] {
    [
        Event::Start(
            Container::RawBlock { format: "html" },
            attr.unwrap_or(Attributes::new()),
        ),
        Event::Str(html.into()),
        Event::End(Container::RawBlock { format: "html" }),
    ]
}

/// Iterator adapter rewriting some djot elements to specific HTML (emitted as raw HTML elements).
/// This is very ugly, but jotdown does not have an AST yet:
/// https://github.com/hellux/jotdown/issues/17
struct Rewrite<'a, I: Iterator<Item = Event<'a>>, T> {
    iter: I,
    next_events: VecDeque<Event<'a>>,
    inner: T,
}

struct RewriteCode;
struct RewriteMath;

#[cfg(feature = "katex")]
mod rewrite_math_opts {
    use std::sync::OnceLock;

    pub(super) static DISPLAY: OnceLock<katex::Opts> = OnceLock::new();
    pub(super) static INLINE: OnceLock<katex::Opts> = OnceLock::new();
}

struct RewriteImages<'a> {
    images: &'a HashMap<String, types::Images>,
}
struct RewriteFigures;

impl<'a, I: Iterator<Item = Event<'a>>, T> Rewrite<'a, I, T> {
    fn new(iter: I, inner: T) -> Self {
        Self {
            iter,
            next_events: VecDeque::with_capacity(3),
            inner,
        }
    }
}

impl<'a, I: Iterator<Item = Event<'a>>> Iterator for Rewrite<'a, I, RewriteCode> {
    type Item = anyhow::Result<Event<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.next_events.is_empty() {
            return Some(Ok(self.next_events.pop_front().unwrap()));
        }

        let event = self.iter.next()?;

        match event {
            Event::Start(Container::CodeBlock { language }, attr) => {
                let code = render_to_raw_string(iter_container_from_inside(&mut self.iter));
                if matches!(language, "" | "plain" | "text" | "plaintext") {
                    let code = html_attr_encode(&code);
                    self.next_events.extend(write_raw_html_inline(
                        format!(r#"<pre><code data-lang={language}>{code}</code></pre>"#),
                        Some(attr),
                    ));
                } else {
                    match highlight::highlight(&code, language) {
                        Ok(highlighted) => {
                            self.next_events.extend(write_raw_html_inline(
                            format!(r#"<pre class="highlight"><code data-lang={language}>{highlighted}</code></pre>"#),
                            Some(attr),
                        ));
                        }
                        Err(highlight::Error::InvalidLanguage) => {
                            log::warn!("an invalid highlight language was requested: {language}");
                            let code = html_attr_encode(&code);
                            self.next_events.extend(write_raw_html_inline(
                                format!(r#"<pre><code data-lang={language}>{code}</code></pre>"#),
                                Some(attr),
                            ));
                        }
                        Err(highlight::Error::Other) => {
                            return Some(Err(anyhow::anyhow!(
                                "an unexpected highlighting error occurred"
                            )))
                        }
                    }
                }

                Some(Ok(self.next_events.pop_front().unwrap()))
            }
            event => Some(Ok(event)),
        }
    }
}

impl<'a, I: Iterator<Item = Event<'a>>> Iterator for Rewrite<'a, I, RewriteMath> {
    type Item = anyhow::Result<Event<'a>>;

    #[cfg(not(any(feature = "katex", feature = "latex2mathml")))]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(Ok)
    }

    #[cfg(any(feature = "katex", feature = "latex2mathml"))]
    fn next(&mut self) -> Option<Self::Item> {
        if !self.next_events.is_empty() {
            return Some(Ok(self.next_events.pop_front().unwrap()));
        }

        let event = self.iter.next()?;

        match event {
            Event::Start(Container::Math { display }, attr) => {
                let math = render_to_raw_string(iter_container_from_inside(&mut self.iter));

                #[cfg(feature = "katex")]
                let math = {
                    // todo: build only once
                    let opts = if display {
                        rewrite_math_opts::DISPLAY.get_or_init(|| {
                            katex::Opts::builder()
                                .display_mode(true)
                                .output_type(katex::OutputType::Mathml)
                                .build()
                                .unwrap()
                        })
                    } else {
                        rewrite_math_opts::INLINE.get_or_init(|| {
                            katex::Opts::builder()
                                .display_mode(false)
                                .output_type(katex::OutputType::Mathml)
                                .build()
                                .unwrap()
                        })
                    };
                    // this is very slow
                    match katex::render_with_opts(&math, &opts) {
                        Err(err) => return Some(Err(err.into())),
                        Ok(math) => math,
                    }
                };

                #[cfg(feature = "latex2mathml")]
                let math = match latex2mathml::latex_to_mathml(
                    &math,
                    if display {
                        latex2mathml::DisplayStyle::Block
                    } else {
                        latex2mathml::DisplayStyle::Inline
                    },
                ) {
                    Ok(math) => math,
                    Err(err) => return Some(Err(err.into())),
                };

                if display {
                    self.next_events.extend(write_raw_html_block(
                        format!(r#"<span class="math display">{math}</span>"#),
                        Some(attr),
                    ))
                } else {
                    self.next_events.extend(write_raw_html_inline(
                        format!(r#"<span class="math">{math}</span>"#),
                        Some(attr),
                    ));
                }

                Some(Ok(self.next_events.pop_front().unwrap()))
            }
            event => Some(Ok(event)),
        }
    }
}

impl<'a, 'b, I: Iterator<Item = Event<'a>>> Iterator for Rewrite<'a, I, RewriteImages<'b>> {
    type Item = anyhow::Result<Event<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.next_events.is_empty() {
            return Some(Ok(self.next_events.pop_front().unwrap()));
        }

        let event = self.iter.next()?;

        match event {
            Event::Start(Container::Image(link, _slt), attr) => {
                let images = match self.inner.images.get(link.as_ref()) {
                    Some(images) => images,
                    None => {
                        eprintln!("Warning, could not find image: {}", link.as_ref());
                        return Some(Ok(Event::Blankline));
                    }
                };

                let alt = render_to_raw_string(iter_container_from_inside(&mut self.iter));
                let srcset = images
                    .original_width
                    .map(|width| format!("/{} {width}w", images.original.to_str().unwrap()));
                let style = images
                    .original_width
                    .map(|width| format!("max-width: calc(min(100%, {}px))", width));
                let img = format_open_tag(
                    "img",
                    [
                        Some((
                            "src",
                            format!("/{}", images.original.to_str().unwrap()).as_str(),
                        )),
                        srcset.as_ref().map(|srcset| ("srcset", srcset.as_str())),
                        Some(("alt", &alt)),
                        style.as_ref().map(|style| ("style", style.as_str())),
                    ]
                    .into_iter()
                    .filter_map(identity),
                    TagCloseKind::Open,
                );
                self.next_events
                    .extend(write_raw_html_inline(img, Some(attr)));
                Some(Ok(self.next_events.pop_front().unwrap()))
            }
            event => Some(Ok(event)),
        }
    }
}

impl<'a, I: Iterator<Item = Event<'a>>> Iterator for Rewrite<'a, I, RewriteFigures> {
    type Item = anyhow::Result<Event<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.next_events.is_empty() {
            return Some(Ok(self.next_events.pop_front().unwrap()));
        }

        let event = self.iter.next()?;

        match event {
            Event::Start(Container::Div { class: "figure" }, attr) => {
                let mut events = iter_container_from_inside(&mut self.iter).peekable();
                self.next_events
                    .extend(write_raw_html_block("<figure>", Some(attr)));
                self.next_events.extend(iter_container(&mut events));
                if events.peek().is_some() {
                    self.next_events
                        .extend(write_raw_html_inline("<figcaption>", None));
                    self.next_events.extend(events);
                    self.next_events
                        .extend(write_raw_html_inline("</figcaption></figure>", None));
                } else {
                    self.next_events
                        .extend(write_raw_html_block("</figure>", None));
                }
                self.next_events
                    .push_back(Event::End(Container::RawInline { format: "html" }));

                Some(Ok(self.next_events.pop_front().unwrap()))
            }
            event => Some(Ok(event)),
        }
    }
}

/// Parse title from a Djot article. The title events are removed from the iter.
pub fn parse_and_render_title(events: &mut Vec<Event<'_>>) -> anyhow::Result<Option<String>> {
    let html_renderer = jotdown::html::Renderer::default();
    let mut title = None;

    let consumed = std::cell::Cell::new(0);
    let mut iter = events
        .iter()
        .cloned()
        .map(|e| {
            consumed.set(consumed.get() + 1);
            e
        })
        .peekable();

    // Skip all initial blank lines
    while matches!(iter.peek(), Some(Event::Blankline)) {
        let _ = iter.next();
    }

    if matches!(
        iter.next(),
        Some(Event::Start(Container::Section { id: _ }, _))
    ) {
        let drain_start = consumed.get();
        if matches!(
            iter.next(),
            Some(Event::Start(
                Container::Heading {
                    level: 1,
                    has_section: _,
                    id: _
                },
                _
            ))
        ) {
            let title_events = iter_container_from_inside(&mut iter);
            let mut title_ = String::new();
            html_renderer.push(title_events, &mut title_)?;
            title_.truncate(title_.trim_end().len());
            title = Some(title_);
        }

        events.drain(drain_start..consumed.get());
    }

    Ok(title)
}

/// Render Djot to HTML. This returns an initial part of the entry and the rest. The two parts are
/// split at a paragraph containing exactly the string `-more-`. If no such paragraph exists, the
/// second part is the empty string.
pub fn render(
    events: Vec<Event<'_>>,
    images: &HashMap<String, types::Images>,
) -> anyhow::Result<(String, String)> {
    let html_renderer = jotdown::html::Renderer::default();

    let split_idx: Option<usize> = {
        let mut found = None;
        for idx in 0..events.len() {
            if !matches!(events[idx], Event::Start(Container::Paragraph, _)) {
                continue;
            }
            if let Event::Str(val) = &events[idx + 1] {
                if val.as_ref() != "-more-" {
                    continue;
                }
            }
            if !matches!(events[idx + 2], Event::End(Container::Paragraph)) {
                continue;
            }

            found = Some(idx);
            break;
        }

        found
    };

    let (summary_events, rest_events) = match split_idx {
        Some(split_idx) => {
            let mut events = events;
            let rest_events = events.split_off(split_idx + 3);
            events.truncate(events.len() - 3);
            (events, rest_events)
        }
        None => (events, vec![]),
    };

    let mut summary = String::new();
    let mut rest = String::new();

    fn rewrite<'e>(
        iter: impl IntoIterator<Item = Event<'e>>,
        images: &HashMap<String, types::Images>,
    ) -> anyhow::Result<Vec<Event<'e>>> {
        let iter = iter.into_iter();

        let iter = Rewrite::new(iter, RewriteCode);
        let iter = utils::process_results_iter(iter, |iter| Rewrite::new(iter, RewriteMath));
        let iter =
            utils::process_results_iter(iter, |iter| Rewrite::new(iter, RewriteImages { images }));
        let iter = utils::process_results_iter(iter, |iter| Rewrite::new(iter, RewriteFigures));

        iter.collect::<anyhow::Result<_>>()
    }

    let summary_events = rewrite(summary_events, images)?;
    let rest_events = rewrite(rest_events, images)?;
    html_renderer.push(summary_events.into_iter(), &mut summary)?;
    html_renderer.push(rest_events.into_iter(), &mut rest)?;

    anyhow::Ok((summary, rest))
}

fn render_to_raw_string<'a>(events: impl Iterator<Item = Event<'a>>) -> String {
    let mut string = String::new();

    for event in events {
        match event {
            Event::Str(val) => string.push_str(val.as_ref()),
            Event::Ellipsis => string.push('…'),
            Event::EnDash => string.push('–'),
            Event::EmDash => string.push('—'),
            Event::NonBreakingSpace => string.push(' '),
            Event::Symbol(val) => {
                string.push(':');
                string.push_str(val.as_ref());
                string.push(':')
            }
            _ => {}
        }
    }

    string.shrink_to_fit();
    string
}

#[cfg(test)]
mod tests {
    #[test]
    fn html_attr_encode() {
        use super::html_attr_encode;

        assert!(matches!(
            html_attr_encode("no encoding"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(html_attr_encode("").as_ref(), "");
        assert_eq!(html_attr_encode("Hello World").as_ref(), "Hello World");
        assert_eq!(
            html_attr_encode("cows & cows & cows").as_ref(),
            "cows &amp; cows &amp; cows"
        );
        assert_eq!(
            html_attr_encode(r#"<span class="Hello World">!@# the answer is 43-1</span>"#).as_ref(),
            r#"&lt;span class=&quot;Hello World&quot;&gt;!@# the answer is 43-1&lt;/span&gt;"#
        );
    }
}
