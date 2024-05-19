//! Intermediate representation of entry markup. This is used for some preprocessing and is
//! rendered to HTML.

use bumpalo::Bump;
use std::{borrow::Cow, collections::HashMap, fmt::Write};

use bitvec::vec::BitVec;

use crate::{highlight, types};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("An IO error occurred")]
    Io(#[from] std::io::Error),
    #[error("A formatting error occurred")]
    Fmt(#[from] std::fmt::Error),
    #[error("An error occurred")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Block element attributes. This struct keeps attributes ordered deterministically by their key.
/// Equality of attribute structs is invariant over key insertion order.
#[derive(Clone, Debug)]
pub struct Attributes<'s> {
    attributes: Vec<(Cow<'s, str>, AttributeValue<'s>)>,
}

impl<'s> Attributes<'s> {
    pub fn new() -> Self {
        Attributes { attributes: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Attributes {
            attributes: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.attributes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[allow(unused)]
    pub fn get(&self, attribute: impl Into<Cow<'s, str>>) -> Option<&AttributeValue<'s>> {
        let attribute = attribute.into();
        self.attributes
            .iter()
            .find_map(|(attribute_, value)| (*attribute_ == attribute).then(|| value))
    }

    pub fn push(&mut self, attribute: impl Into<Cow<'s, str>>, value: impl Into<AttributeValue<'s>>) {
        let attribute = attribute.into();

        if let Some(entry) = self
            .attributes
            .iter_mut()
            .find_map(|(attribute_, value)| (*attribute_ == attribute).then(|| value))
        {
            *entry = value.into();
        } else {
            // insertion to keep attributes sorted
            let mut idx = 0;
            for (attribute_, _) in &self.attributes {
                if *attribute_ > attribute {
                    break;
                }
                idx += 1;
            }
            self.attributes.insert(idx, (attribute, value.into()));
        }
    }

    fn into_iter(self) -> impl Iterator<Item = (Cow<'s, str>, AttributeValuePlusFmt<'s>)> {
        self.attributes.into_iter().map(|(attr, val)| (attr, val.into()))
    }
}

impl<'s> From<jotdown::Attributes<'s>> for Attributes<'s> {
    fn from(attributes: jotdown::Attributes<'s>) -> Self {
        let mut attributes_ = Attributes::with_capacity(attributes.len());

        for (attr, val) in attributes {
            attributes_.push(attr, val);
        }

        attributes_
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum AttributeValue<'s> {
    Jotdown(jotdown::AttributeValue<'s>),
    Raw(Cow<'s, str>),
}

impl<'s> std::fmt::Debug for AttributeValue<'s> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttributeValue::Jotdown(val) => write!(f, "{val:?}"),
            AttributeValue::Raw(val) => write!(f, "{val:?}"),
        }
    }
}

impl<'s> From<jotdown::AttributeValue<'s>> for AttributeValue<'s> {
    fn from(value: jotdown::AttributeValue<'s>) -> Self {
        AttributeValue::Jotdown(value)
    }
}

impl<'s> From<Cow<'s, str>> for AttributeValue<'s> {
    fn from(value: Cow<'s, str>) -> Self {
        AttributeValue::Raw(value)
    }
}

impl<'s> From<&'s str> for AttributeValue<'s> {
    fn from(value: &'s str) -> Self {
        AttributeValue::Raw(value.into())
    }
}

impl<'s> From<String> for AttributeValue<'s> {
    fn from(value: String) -> Self {
        AttributeValue::Raw(value.into())
    }
}

impl<'s, T> From<Option<T>> for AttributeValue<'s>
where
    AttributeValue<'s>: From<T>,
{
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => value.into(),
            None => "".into(),
        }
    }
}

/// Store attribute values that are to be written escaped. Only meant for internal usage here to
/// prevent some allocations.
enum AttributeValuePlusFmt<'s> {
    Jotdown(jotdown::AttributeValue<'s>),
    Raw(Cow<'s, str>),
    FmtArguments(std::fmt::Arguments<'s>),
    Display(&'s dyn std::fmt::Display),
}

impl AttributeValuePlusFmt<'_> {
    fn write_escaped(&self, buf: &mut String) {
        match self {
            AttributeValuePlusFmt::Jotdown(val) => write!(buf, "{val}").expect("infallible"),
            AttributeValuePlusFmt::Raw(val) => pulldown_cmark_escape::escape_html(buf, val).expect("infallible"),
            AttributeValuePlusFmt::FmtArguments(val) => write!(buf, "{val}").expect("infallible"),
            AttributeValuePlusFmt::Display(val) => write!(buf, "{val}").expect("infallible"),
        }
    }
}

impl<'s> From<AttributeValue<'s>> for AttributeValuePlusFmt<'s> {
    fn from(value: AttributeValue<'s>) -> Self {
        match value {
            AttributeValue::Jotdown(val) => AttributeValuePlusFmt::Jotdown(val),
            AttributeValue::Raw(val) => AttributeValuePlusFmt::Raw(val),
        }
    }
}

impl<'s> From<jotdown::AttributeValue<'s>> for AttributeValuePlusFmt<'s> {
    fn from(value: jotdown::AttributeValue<'s>) -> Self {
        AttributeValuePlusFmt::Jotdown(value)
    }
}

impl<'s> From<Cow<'s, str>> for AttributeValuePlusFmt<'s> {
    fn from(value: Cow<'s, str>) -> Self {
        AttributeValuePlusFmt::Raw(value)
    }
}

impl<'s> From<&'s str> for AttributeValuePlusFmt<'s> {
    fn from(value: &'s str) -> Self {
        AttributeValuePlusFmt::Raw(value.into())
    }
}

impl<'s> From<String> for AttributeValuePlusFmt<'s> {
    fn from(value: String) -> Self {
        AttributeValuePlusFmt::Raw(value.into())
    }
}

impl<'s> From<std::fmt::Arguments<'s>> for AttributeValuePlusFmt<'s> {
    fn from(value: std::fmt::Arguments<'s>) -> Self {
        AttributeValuePlusFmt::FmtArguments(value)
    }
}

impl<'s> From<&'s std::fmt::Arguments<'s>> for AttributeValuePlusFmt<'s> {
    fn from(value: &'s std::fmt::Arguments<'s>) -> Self {
        AttributeValuePlusFmt::Display(value)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Alignment {
    Unspecified,
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub enum OrderedListNumbering {
    Decimal,
    AlphaLower,
    AlphaUpper,
    RomanLower,
    RomanUpper,
}

#[derive(Clone, Copy, Debug)]
pub enum ListKind {
    Unordered,
    Ordered {
        numbering: OrderedListNumbering,
        start: u64,
    },
    Task,
}

#[derive(Clone, Copy, Debug)]
pub enum MathKind {
    Display,
    Inline,
}

#[derive(Clone, Debug)]
pub enum Container<'s> {
    Blockquote,

    DescriptionList,
    DescriptionTerm,
    DescriptionDetails,

    Heading { level: u16, id: Cow<'s, str> },
    Section { id: Cow<'s, str> },
    Div,
    Paragraph,

    Link { destination: Cow<'s, str> },

    List { kind: ListKind, tight: bool },
    ListItem,
    TaskListItem { checked: bool },

    Table,
    TableHead,
    TableBody,
    TableRow,
    TableCell { alignment: Alignment, head: bool },

    Footnote { label: Cow<'s, str> },

    Other { tag: Cow<'s, str> },
}

#[derive(Clone, Debug)]
pub enum ContainerEnd<'s> {
    Blockquote,

    DescriptionList,
    DescriptionTerm,
    DescriptionDetails,

    Heading { level: u16 },
    Section,
    Div,
    Paragraph,

    Link,

    List { kind: ListKind },
    ListItem,
    TaskListItem,

    Table,
    TableHead,
    TableBody,
    TableRow,
    TableCell { head: bool },

    Footnote,

    Other { tag: Cow<'s, str> },
}

#[derive(Clone, Debug)]
pub enum Event<'s> {
    Start {
        container: Container<'s>,
        attributes: Attributes<'s>,
    },
    End {
        container: ContainerEnd<'s>,
    },
    Str(Cow<'s, str>),
    Image {
        destination: Cow<'s, str>,
        alt: Cow<'s, str>,
        attributes: Attributes<'s>,
    },
    CodeBlock {
        language: Cow<'s, str>,
        code: Cow<'s, str>,
        attributes: Attributes<'s>,
    },
    Math {
        kind: MathKind,
        math: Cow<'s, str>,
        attributes: Attributes<'s>,
    },
    HtmlInline {
        content: Cow<'s, str>,
        attributes: Attributes<'s>,
    },
    HtmlBlock {
        content: Cow<'s, str>,
        attributes: Attributes<'s>,
    },
    TagWithAttribute {
        tag: Cow<'s, str>,
        attributes: Attributes<'s>,
    },

    FootnoteReference {
        reference: Cow<'s, str>,
    },
}

#[derive(Clone, Copy)]
enum FootnoteState {
    Missing,
    Defined,
}

#[derive(Clone)]
struct Footnote {
    buf: String,
    number: std::num::NonZeroUsize,
    state: FootnoteState,
}

enum WriteTarget<'w> {
    Buf,
    FootnotesBuf { label: Cow<'w, str> },
}

struct Writer<'w> {
    buf: &'w mut String,

    write_target: WriteTarget<'w>,

    /// state stacks
    list_tightness: BitVec,

    /// Footnote reference numbering and rendered footnote buffers
    footnotes: HashMap<Cow<'w, str>, Footnote>,
}

impl<'w> Writer<'w> {
    fn new(buf: &'w mut String) -> Self {
        Writer {
            buf,

            write_target: WriteTarget::Buf,

            list_tightness: BitVec::new(),

            footnotes: HashMap::new(),
        }
    }

    #[inline]
    fn in_tight_list(&self) -> bool {
        *self.list_tightness.last().as_deref().unwrap_or(&false)
    }

    /// Calls the provided function with the correct buffer to write to.
    #[inline]
    fn with_buf<R>(&mut self, f: impl FnOnce(&mut String) -> R) -> R {
        let buf = match self.write_target {
            WriteTarget::Buf => &mut self.buf,
            WriteTarget::FootnotesBuf { ref label } => self
                .footnotes
                .get_mut(label)
                .map(|footnote| &mut footnote.buf)
                .expect("called with unregistered footnote label"),
        };

        f(buf)
    }

    /// Start a new line if we're not on a new line yet.
    #[inline]
    fn ensure_newline(&mut self) -> std::io::Result<()> {
        self.with_buf(|buf| {
            let wrote_newline = buf.is_empty() || buf.ends_with("\n");
            if !wrote_newline {
                buf.push('\n');
            }
        });
        Ok(())
    }

    #[inline]
    fn write(&mut self, s: &str) -> std::io::Result<()> {
        self.with_buf(|buf| buf.push_str(s));
        Ok(())
    }

    #[inline]
    fn write_on_new_line(&mut self, s: &str) -> std::io::Result<()> {
        self.ensure_newline()?;
        self.write(s)?;
        Ok(())
    }

    #[inline]
    fn write_tag_with_attributes<'a>(
        &mut self,
        tag: &str,
        attributes: impl IntoIterator<Item = (Cow<'a, str>, AttributeValuePlusFmt<'a>)>,
    ) -> Result<()> {
        self.with_buf(|buf| {
            buf.push('<');
            buf.push_str(tag);
            for (attr, val) in attributes.into_iter() {
                buf.push(' ');
                buf.push_str(&attr);
                buf.push_str(r#"=""#);
                val.write_escaped(buf);
                buf.push('"');
            }
            buf.push('>');
        });

        Ok(())
    }

    #[inline]
    fn write_tag_with_attributes_on_new_line<'a>(
        &mut self,
        tag: &str,
        attributes: impl IntoIterator<Item = (Cow<'a, str>, AttributeValuePlusFmt<'a>)>,
    ) -> Result<()> {
        self.ensure_newline()?;
        self.write_tag_with_attributes(tag, attributes)?;
        Ok(())
    }

    /// Register a footnote reference and get its number.
    fn register_footnote_reference(&mut self, label: &Cow<'w, str>) -> usize {
        let number = std::num::NonZeroUsize::new(self.footnotes.len() + 1).unwrap();
        // if https://github.com/rust-lang/rust/issues/56167 is stabilized, clone can be done only
        // when needed
        self.footnotes
            .entry(label.clone())
            .or_insert(Footnote {
                number,
                state: FootnoteState::Missing,
                buf: String::new(),
            })
            .number
            .into()
    }

    /// Register a footnote definition and get its number.
    fn register_footnote_definition(&mut self, label: &Cow<'w, str>) -> usize {
        let number = std::num::NonZeroUsize::new(self.footnotes.len().saturating_add(1)).unwrap();
        // if https://github.com/rust-lang/rust/issues/56167 is stabilized, clone can be done only
        // when needed
        let entry = self.footnotes.entry(label.clone()).or_insert(Footnote {
            number,
            state: FootnoteState::Missing,
            buf: String::new(),
        });
        if matches!(entry.state, FootnoteState::Defined) {
            log::warn!("Footnote defined multiple times: {label}");
        }
        entry.state = FootnoteState::Defined;
        entry.number.into()
    }

    fn start_tag<'s>(&mut self, bump: &Bump, container: Container<'w>, attributes: Attributes<'s>) -> Result<()> {
        use std::fmt::Write;

        match container {
            // Container::HtmlBlock => Ok(()),
            Container::Blockquote => {
                self.write_tag_with_attributes_on_new_line("blockquote", attributes.into_iter())?
            }

            Container::DescriptionList => self.write_tag_with_attributes_on_new_line("dl", attributes.into_iter())?,
            Container::DescriptionTerm => self.write_tag_with_attributes_on_new_line("dt", attributes.into_iter())?,
            Container::DescriptionDetails => {
                self.write_tag_with_attributes_on_new_line("dd", attributes.into_iter())?
            }

            Container::Section { id } => self.write_tag_with_attributes_on_new_line(
                "section".into(),
                attributes.into_iter().chain([("id".into(), id.into())]),
            )?,
            Container::Heading { level, id } => {
                let tag = match level {
                    1 => "h1",
                    2 => "h2",
                    3 => "h3",
                    4 => "h4",
                    5 => "h5",
                    _ => "h6",
                };
                self.write_tag_with_attributes_on_new_line(tag, attributes.into_iter())?;
                self.write_tag_with_attributes("a", [("href".into(), (&format_args!("#{id}")).into())])?;
            }
            Container::Div => {
                self.write_tag_with_attributes_on_new_line("div", attributes.into_iter())?;
            }
            Container::Paragraph => {
                if !self.in_tight_list() {
                    self.write_tag_with_attributes_on_new_line("p", attributes.into_iter())?
                }
            }

            Container::Link { destination } => {
                // TODO: escape
                self.write_tag_with_attributes_on_new_line(
                    "a",
                    attributes.into_iter().chain([("href".into(), destination.into())]),
                )?
            }

            Container::List { kind, tight } => {
                self.list_tightness.push(tight);
                match kind {
                    ListKind::Unordered => self.write_tag_with_attributes_on_new_line("ul", attributes.into_iter())?,
                    ListKind::Ordered { numbering, start } => {
                        let r#type = if matches!(numbering, OrderedListNumbering::Decimal) {
                            None
                        } else {
                            Some(match numbering {
                                OrderedListNumbering::Decimal => unreachable!(),
                                OrderedListNumbering::AlphaLower => "a",
                                OrderedListNumbering::AlphaUpper => "A",
                                OrderedListNumbering::RomanLower => "i",
                                OrderedListNumbering::RomanUpper => "I",
                            })
                        };
                        let start = if start == 1 {
                            None
                        } else {
                            let mut s = bumpalo::collections::String::new_in(bump);
                            write!(s, "{start}")?;
                            Some(s.into_bump_str())
                        };
                        self.write_tag_with_attributes_on_new_line(
                            "ol",
                            attributes
                                .into_iter()
                                .chain(r#type.map(|r#type| ("type".into(), r#type.into())))
                                .chain(start.map(|start| ("start".into(), start.into()))),
                        )?;
                    }
                    ListKind::Task => self.write_tag_with_attributes_on_new_line(
                        "ul",
                        attributes.into_iter().chain([("class".into(), "task-list".into())]),
                    )?,
                }
            }
            Container::ListItem => self.write_tag_with_attributes_on_new_line("li", attributes.into_iter())?,
            Container::TaskListItem { checked } => self.write_tag_with_attributes_on_new_line(
                "li",
                attributes.into_iter().chain([
                    ("class".into(), (if checked { "checked" } else { "unchecked" }).into()),
                    ("data-checked".into(), (if checked { "true" } else { "false" }).into()),
                ]),
            )?,
            Container::Table => self.write_tag_with_attributes_on_new_line("table", attributes.into_iter())?,
            Container::TableHead => self.write_tag_with_attributes_on_new_line("thead", attributes.into_iter())?,
            Container::TableBody => self.write_tag_with_attributes_on_new_line("tbody", attributes.into_iter())?,
            Container::TableRow => self.write_tag_with_attributes_on_new_line("tr", attributes.into_iter())?,
            Container::TableCell { alignment, head } => {
                let tag = if head { "th" } else { "td" };
                let style = match alignment {
                    Alignment::Unspecified => None,
                    Alignment::Left => Some(("style".into(), AttributeValuePlusFmt::from("text-align: left;"))),
                    Alignment::Center => Some(("style".into(), AttributeValuePlusFmt::from("text-align: center;"))),
                    Alignment::Right => Some(("style".into(), AttributeValuePlusFmt::from("text-align: right;"))),
                };
                self.write_tag_with_attributes_on_new_line(tag, style)?
            }

            Container::Footnote { label } => {
                let num = self.register_footnote_definition(&label);
                self.write_target = WriteTarget::FootnotesBuf { label: label.clone() };
                self.write_tag_with_attributes_on_new_line(
                    "li",
                    attributes.into_iter().chain([
                        ("class".into(), "footnote-definition".into()),
                        ("id".into(), (&format_args!("fn-{num}")).into()),
                        ("role".into(), "doc-footnote".into()),
                    ]),
                )?;
            }

            Container::Other { tag } => {
                self.write_tag_with_attributes_on_new_line(tag.as_ref(), attributes.into_iter())?
            }
        }

        Ok(())
    }

    fn end_tag(&mut self, _bump: &Bump, container: ContainerEnd) -> Result<()> {
        match container {
            ContainerEnd::Blockquote => self.write("</blockquote>")?,

            ContainerEnd::DescriptionList => self.write_on_new_line("</dl>\n")?,
            ContainerEnd::DescriptionTerm => self.write("</dt>")?,
            ContainerEnd::DescriptionDetails => self.write("</dd>")?,

            ContainerEnd::Heading { level } => {
                let write = match level {
                    1 => "</a></h1>\n",
                    2 => "</a></h2>\n",
                    3 => "</a></h3>\n",
                    4 => "</a></h4>\n",
                    5 => "</a></h5>\n",
                    _ => "</a></h6>\n",
                };
                self.write(write)?;
            }
            ContainerEnd::Section => self.write("</section>\n")?,
            ContainerEnd::Div => self.write("</div>\n")?,
            ContainerEnd::Paragraph => {
                if !self.in_tight_list() {
                    self.write("</p>\n")?
                }
            }
            ContainerEnd::Link => self.write("</a>")?,

            ContainerEnd::List { kind } => {
                self.list_tightness.pop();
                match kind {
                    ListKind::Unordered | ListKind::Task => self.write("</ul>\n")?,
                    ListKind::Ordered { numbering: _, start: _ } => self.write("</ol>\n")?,
                }
            }
            ContainerEnd::ListItem | ContainerEnd::TaskListItem => self.write("</li>\n")?,

            ContainerEnd::Table => self.write("</table>\n")?,
            ContainerEnd::TableHead => self.write("</thead>\n")?,
            ContainerEnd::TableBody => self.write("</tbody>\n")?,
            ContainerEnd::TableRow => self.write("</tr>\n")?,
            ContainerEnd::TableCell { head } => {
                let tag = if head { "th" } else { "td" };
                self.write("</")?;
                self.write(tag)?;
                self.write(">\n")?;
            }

            ContainerEnd::Footnote => {
                self.write_on_new_line("</li>\n")?;
                self.write_target = WriteTarget::Buf;
            }

            ContainerEnd::Other { tag } => {
                self.write("</")?;
                self.write(tag.as_ref())?;
                self.write(">")?;
            }
        }

        Ok(())
    }
}

pub fn push_html<'s>(
    buf: &mut String,
    mut iter: impl Iterator<Item = Event<'s>>,
    images: &HashMap<String, types::Images>,
) -> Result<()> {
    let mut bump = Bump::new();
    let mut writer = Writer::new(buf);

    while let Some(ev) = iter.next() {
        match ev {
            Event::Start { container, attributes } => {
                writer.start_tag(&bump, container, attributes)?;
            }
            Event::End { container } => writer.end_tag(&bump, container)?,
            Event::Str(str) => {
                writer.with_buf(|buf| pulldown_cmark_escape::escape_html_body_text(buf, str.as_ref()))?
            }
            Event::Image {
                destination,
                alt,
                attributes,
            } => {
                let images = match images.get(destination.as_ref()) {
                    Some(images) => images,
                    None => {
                        eprintln!("Warning, could not find image: {destination}");
                        continue;
                    }
                };

                let mut srcset: Option<&'_ str> = None;
                let mut style: Option<&'_ str> = None;

                if let Some(width) = images.original_width {
                    let mut srcset_ = bumpalo::collections::String::new_in(&bump);
                    write!(srcset_, "/{} {width}w", images.original.to_str().unwrap())?;
                    if let Some(ref link) = images.x_1536 {
                        write!(srcset_, ",/{} 1536w", link.to_str().unwrap())?;
                    }
                    if let Some(ref link) = images.x_768 {
                        write!(srcset_, ",/{} 768w", link.to_str().unwrap())?;
                    }
                    srcset = Some(srcset_.into_bump_str());
                    style = Some(bumpalo::format!(in &bump, "max-width: calc(min(100%, {}px))", width).into_bump_str());
                }

                writer.write_tag_with_attributes_on_new_line(
                    "img",
                    attributes
                        .into_iter()
                        .chain([("src".into(), destination.into())])
                        .chain(srcset.map(|srcset| ("srcset".into(), srcset.into())))
                        .chain(style.map(|style| ("style".into(), style.into())))
                        .chain((alt == "").then(|| ("alt".into(), alt.into()))),
                )?
            }
            Event::CodeBlock {
                language,
                code,
                attributes,
            } => match highlight::highlight(&code, &language)? {
                highlight::Highlighted::Plain(plaintext) => {
                    writer.write_tag_with_attributes_on_new_line("pre", attributes.into_iter())?;
                    writer.write_on_new_line("<code>")?;
                    writer.write_on_new_line(&plaintext)?;
                    writer.write_on_new_line("</code>\n</pre>")?;
                }
                highlight::Highlighted::Highlighted { language, highlighted } => {
                    writer.write_tag_with_attributes_on_new_line(
                        "pre",
                        attributes.into_iter().chain([("class".into(), "highlight".into())]),
                    )?;
                    writer.write_tag_with_attributes_on_new_line("code", [("data-lang".into(), language.into())])?;
                    writer.write_on_new_line(&highlighted)?;
                    writer.write_on_new_line("</code>\n</pre>")?;
                }
            },

            #[allow(unused_variables)]
            Event::Math { kind, math, attributes } => {
                writer.write_tag_with_attributes_on_new_line(
                    "span",
                    attributes.into_iter().chain([("class".into(), "math".into())]),
                )?;
                #[cfg(any(feature = "katex", feature = "latex2mathml"))]
                {
                    writer.write(&render_latex(&math, &kind)?)?;
                }
                #[cfg(not(any(feature = "katex", feature = "latex2mathml")))]
                {
                    writer.with_buf(|buf| pulldown_cmark_escape::escape_html_body_text(buf, &math))?;
                }
                writer.write("</span>")?;
            }
            Event::HtmlBlock { content, attributes } => {
                writer.write_tag_with_attributes_on_new_line("div", attributes.into_iter())?;
                writer.write(&content)?;
                writer.write("</div>\n")?
            }
            Event::HtmlInline { content, attributes } => {
                if attributes.is_empty() {
                    writer.write(&content)?;
                } else {
                    writer.write_tag_with_attributes_on_new_line("span", attributes.into_iter())?;
                    writer.write(&content)?;
                    writer.write("</span>\n")?
                }
            }
            Event::TagWithAttribute { tag, attributes } => {
                writer.write_tag_with_attributes_on_new_line(tag.as_ref(), attributes.into_iter())?
            }

            Event::FootnoteReference { reference } => {
                let num = writer.register_footnote_reference(&reference);
                writer.write("<sup class=\"footnote-reference\">")?;
                writer.write_tag_with_attributes(
                    "a",
                    [
                        ("role".into(), "doc-noteref".into()),
                        ("href".into(), (&format_args!("#fn-{num}")).into()),
                    ],
                )?;
                writer.with_buf(|buf| write!(buf, "{num}"))?;
                writer.write("</a>")?;
                writer.write_on_new_line("</sup>")?;
            }
        }
        bump.reset();
    }

    if !writer.footnotes.is_empty() {
        writer.write("<hr>\n<aside class=\"footnotes\" role=\"doc-endnotes\">\n<ol>\n")?;
        let footnotes = {
            let mut footnotes: Vec<_> = writer.footnotes.values().cloned().collect();
            footnotes.sort_by_key(|f| f.number);
            footnotes
        };
        for footnote in footnotes {
            match footnote.state {
                FootnoteState::Defined => writer.with_buf(|buf| buf.push_str(&footnote.buf)),
                FootnoteState::Missing => {
                    let num = footnote.number;

                    {
                        let key = writer
                            .footnotes
                            .iter()
                            .find(|(_, footnote)| footnote.number == num)
                            .expect("invariant")
                            .0;
                        log::warn!("footnote definition missing: {key}");
                    }

                    writer.write_tag_with_attributes_on_new_line(
                        "li",
                        [
                            ("class".into(), "footnote-definition".into()),
                            ("id".into(), (&format_args!("fn-{num}")).into()),
                            ("role".into(), "doc-footnote".into()),
                        ],
                    )?;
                }
            }
        }
        buf.push_str("</ol>\n</aside>\n");
    }

    Ok(())
}

#[cfg(feature = "katex")]
fn render_latex(latex: &str, kind: &MathKind) -> anyhow::Result<String> {
    use std::sync::OnceLock;

    static DISPLAY: OnceLock<katex::Opts> = OnceLock::new();
    static INLINE: OnceLock<katex::Opts> = OnceLock::new();

    let opts = match kind {
        MathKind::Display => DISPLAY.get_or_init(|| {
            katex::Opts::builder()
                .display_mode(true)
                .output_type(katex::OutputType::Mathml)
                .build()
                .unwrap()
        }),
        MathKind::Inline => INLINE.get_or_init(|| {
            katex::Opts::builder()
                .display_mode(false)
                .output_type(katex::OutputType::Mathml)
                .build()
                .unwrap()
        }),
    };

    // this is very slow
    Ok(katex::render_with_opts(latex, opts)?)
}

#[cfg(feature = "latex2mathml")]
fn render_latex(latex: &str, kind: &MathKind) -> anyhow::Result<String> {
    Ok(latex2mathml::latex_to_mathml(
        latex,
        match kind {
            MathKind::Display => latex2mathml::DisplayStyle::Block,
            MathKind::Inline => latex2mathml::DisplayStyle::Inline,
        },
    )?)
}

/// Iterates from an Event::Start to a matching Event::End. The resulting iterator yields all
/// events in between the start and end, skipping over the start and end itself. If the next item
/// in the iterator is not a Event::Start, the resulting iterator is immediately empty.
fn iter_container<'a>(iter: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    let mut depth = 0usize;
    iter.take_while(move |event| match event {
        Event::Start {
            container: _,
            attributes: _,
        } => {
            depth += 1;
            true
        }
        Event::End { container: _ } => {
            depth -= 1;
            depth > 0
        }
        _ => true,
    })
    .skip(1)
}

/// Same as `iter_container` but assumes it Event::Start has already been consumed.
fn iter_container_from_inside<'a>(iter: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    iter_container(
        // prepend a fake start block
        [Event::Start {
            container: Container::Paragraph,
            attributes: Attributes::new(),
        }]
        .into_iter()
        .chain(iter),
    )
}

/// Parse title from a Djot article. The title events are removed from the iter.
pub fn parse_and_render_title(events: &mut Vec<Event<'_>>) -> anyhow::Result<Option<String>> {
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

    // // Skip all initial blank lines
    // while matches!(iter.peek(), Some(Event::Blankline)) {
    //     let _ = iter.next();
    // }

    if matches!(
        iter.next(),
        Some(Event::Start {
            container: Container::Section { id: _ },
            attributes: _
        })
    ) {
        let drain_start = consumed.get();
        if matches!(
            iter.next(),
            Some(Event::Start {
                container: Container::Heading { level: 1, id: _ },
                attributes: _
            })
        ) {
            let title_events = iter_container_from_inside(&mut iter);
            let mut title_ = String::new();
            push_html(&mut title_, title_events, &HashMap::new())?;
            title_.truncate(title_.trim_end().len());
            title = Some(title_);
        }

        events.drain(drain_start..consumed.get());
    }

    Ok(title)
}

/// Rewrites internal links in the format `~/<canonical name>` (e.g. `posts/2024-04-23-something`)
/// to the HTTP URL. Returns the entries this entry links to.
pub fn rewrite_and_emit_internal_links<'entries>(
    events: &mut Vec<Event<'_>>,
    entries_by_name: &HashMap<&str, &'entries types::EntryMetaAndFrontMatter<'entries>>,
) -> anyhow::Result<Vec<&'entries types::EntryMetaAndFrontMatter<'entries>>> {
    let mut internal_links = vec![];

    fn rewrite_link<'entries>(
        old_link: &mut Cow<'_, str>,
        entries_by_name: &HashMap<&str, &'entries types::EntryMetaAndFrontMatter<'entries>>,
    ) -> anyhow::Result<Option<&'entries types::EntryMetaAndFrontMatter<'entries>>> {
        if &old_link[0..2] == "~/" {
            let (link, anchor) = match old_link.find('#') {
                Some(anchor_idx) => (&old_link[2..anchor_idx], &old_link[anchor_idx..]),
                None => (&old_link[2..], ""),
            };

            if let Some(entry) = entries_by_name.get(link) {
                *old_link = Cow::Owned(format!("{}{}", &entry.meta.permalink, anchor));
                return Ok(Some(entry));
            } else {
                anyhow::bail!("Unknown internal link: {old_link}");
            }
        }

        Ok(None)
    }

    for event in events {
        match event {
            Event::Start {
                container: Container::Link { destination },
                attributes: _,
            } => {
                if let Some(entry) = rewrite_link(destination, entries_by_name)? {
                    internal_links.push(entry);
                }
            }
            _ => {}
        }
    }

    Ok(internal_links)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{AttributeValue, Attributes};

    #[test]
    fn attr_alphabetical() {
        let mut attributes = Attributes::new();

        attributes.push("foo", "");
        attributes.push("bar", "");
        attributes.push("qux", "");
        attributes.push("cafe", "");

        let mut iter = attributes.into_iter();
        assert_eq!(iter.next().unwrap().0, Cow::from("bar"));
        assert_eq!(iter.next().unwrap().0, Cow::from("cafe"));
        assert_eq!(iter.next().unwrap().0, Cow::from("foo"));
        assert_eq!(iter.next().unwrap().0, Cow::from("qux"));
        assert!(iter.next().is_none());
    }

    #[test]
    fn attr_override() {
        let mut attributes = Attributes::new();

        attributes.push("foo", "");
        assert_eq!(attributes.get("foo").unwrap(), &AttributeValue::Raw("".into()),);
        attributes.push("foo", "bar");
        assert_eq!(attributes.get("foo").unwrap(), &AttributeValue::Raw("bar".into()),);
    }
}
