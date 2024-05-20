use std::borrow::Cow;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};

use crate::ir_markup::{
    Alignment as IrAlignment, Attributes, Container as IrContainer, ContainerEnd as IrContainerEnd, Event as IrEvent,
    HeadingLevel as IrHeadingLevel, ListKind as IrListKind, OrderedListNumbering as IrOrderedListNumbering,
};

/// Iterates from an Event::Start to a matching Event::End. The resulting iterator yields all
/// events in between the start and end, skipping over the start and end itself. If the next item
/// in the iterator is not a Event::Start, the resulting iterator is immediately empty.
fn iter_container<'a>(iter: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    let mut depth = 0usize;
    iter.take_while(move |event| match event {
        Event::Start(_) => {
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
fn iter_container_from_inside<'a>(iter: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    iter_container(
        // prepend a fake start block
        [Event::Start(Tag::Paragraph)].into_iter().chain(iter),
    )
}

fn render_to_raw_string<'a>(events: impl Iterator<Item = Event<'a>>) -> Cow<'a, str> {
    let mut string: Cow<'a, str> = "".into();

    for event in events {
        match event {
            Event::Text(val) => {
                if string.as_ref() == "" {
                    string = val.into();
                } else {
                    string.to_mut().push_str(val.as_ref())
                }
            }
            _ => {}
        }
    }

    string
}

impl From<pulldown_cmark::HeadingLevel> for IrHeadingLevel {
    fn from(value: pulldown_cmark::HeadingLevel) -> Self {
        match value {
            HeadingLevel::H1 => IrHeadingLevel::H1,
            HeadingLevel::H2 => IrHeadingLevel::H2,
            HeadingLevel::H3 => IrHeadingLevel::H3,
            HeadingLevel::H4 => IrHeadingLevel::H4,
            HeadingLevel::H5 => IrHeadingLevel::H5,
            HeadingLevel::H6 => IrHeadingLevel::H6,
        }
    }
}

enum TableHeadOrBody {
    Head,
    Body,
}

struct Context {
    table_alignment: Vec<pulldown_cmark::Alignment>,
    table_cell_idx: usize,
    table_head_or_body: TableHeadOrBody,
    section_stack: Vec<HeadingLevel>,
}

impl Context {
    fn new() -> Self {
        Context {
            table_alignment: Vec::new(),
            table_cell_idx: 0,
            table_head_or_body: TableHeadOrBody::Head,
            section_stack: Vec::with_capacity(6),
        }
    }
}

fn markdown_to_ir<'s>(mut markdown: impl Iterator<Item = Event<'s>>) -> impl Iterator<Item = IrEvent<'s>> {
    let mut ctx = Context::new();

    // to be replaced by `gen`-blocks
    genawaiter::rc::Gen::new(|co| async move {
        while let Some(ev) = markdown.next() {
            match ev {
                Event::Start(Tag::Paragraph) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Paragraph,
                        attributes: Attributes::new(),
                    })
                    .await;
                }
                Event::End(TagEnd::Paragraph) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Paragraph,
                    })
                    .await;
                }

                Event::Start(Tag::Heading {
                    level,
                    id,
                    classes,
                    attrs,
                }) => {
                    let mut attributes = Attributes::new();
                    if !classes.is_empty() {
                        let class = {
                            let class_len = classes.iter().map(|class| class.len()).sum::<usize>() + classes.len() - 1;
                            let mut class = String::with_capacity(class_len);
                            for class_ in classes {
                                class.push_str(&class_);
                            }
                            class
                        };
                        if !class.is_empty() {
                            attributes.insert("class", class);
                        }
                    }
                    // for (attr, val) in attrs {
                    //     attributes.insert(
                    //         Cow::from(attr),
                    //         val.map(|val| Cow::from(val)).unwrap_or(Cow::Borrowed("")).into(),
                    //     )
                    // }

                    while let Some(&open_section_level) = ctx.section_stack.last() {
                        if level <= open_section_level {
                            ctx.section_stack.pop();

                            co.yield_(IrEvent::End {
                                container: IrContainerEnd::Section,
                            })
                            .await;
                        } else {
                            break;
                        }
                    }
                    ctx.section_stack.push(level);

                    co.yield_(IrEvent::Start {
                        container: IrContainer::Section {
                            id: id.clone().unwrap_or("".into()).into(),
                        },
                        attributes: Attributes::new(),
                    })
                    .await;
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Heading {
                            level: level.into(),
                            id: id.unwrap_or("".into()).into(),
                        },
                        attributes: Attributes::new(),
                    })
                    .await;
                }
                Event::End(TagEnd::Heading(level)) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Heading { level: level.into() },
                    })
                    .await;
                }

                Event::Start(Tag::BlockQuote) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Blockquote,
                        attributes: Attributes::new(),
                    })
                    .await
                }
                Event::End(TagEnd::BlockQuote) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Blockquote,
                    })
                    .await
                }

                Event::Start(Tag::CodeBlock(kind)) => {
                    let language = match kind {
                        CodeBlockKind::Indented => "".into(),
                        CodeBlockKind::Fenced(language) => language,
                    };
                    let code = render_to_raw_string(iter_container_from_inside(&mut markdown));
                    co.yield_(IrEvent::CodeBlock {
                        language: language.into(),
                        code,
                        attributes: Attributes::new(),
                    })
                    .await
                }
                Event::End(TagEnd::CodeBlock) => unreachable!(),

                Event::Start(Tag::HtmlBlock) => {}
                Event::End(TagEnd::HtmlBlock) => {}

                Event::Start(Tag::List(start)) => {
                    let container = match start {
                        Some(start) => IrContainer::List {
                            kind: IrListKind::Ordered {
                                numbering: IrOrderedListNumbering::Decimal,
                                start,
                            },
                            tight: true,
                        },
                        None => IrContainer::List {
                            kind: IrListKind::Unordered,
                            tight: true,
                        },
                    };
                    co.yield_(IrEvent::Start {
                        container,
                        attributes: Attributes::new(),
                    })
                    .await;
                }
                Event::End(TagEnd::List(ordered)) => {
                    let container = if ordered {
                        IrContainerEnd::List {
                            kind: IrListKind::Ordered {
                                numbering: IrOrderedListNumbering::Decimal,
                                start: 0,
                            },
                        }
                    } else {
                        IrContainerEnd::List {
                            kind: IrListKind::Unordered,
                        }
                    };
                    co.yield_(IrEvent::End { container }).await
                }

                Event::Start(Tag::Item) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::ListItem,
                        attributes: Attributes::new(),
                    })
                    .await
                }
                Event::End(TagEnd::Item) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::ListItem,
                    })
                    .await
                }

                Event::Start(Tag::MetadataBlock(_)) => unreachable!(),
                Event::End(TagEnd::MetadataBlock(_)) => unreachable!(),

                Event::Start(Tag::Table(alignment)) => {
                    ctx.table_alignment = alignment;

                    co.yield_(IrEvent::Start {
                        container: IrContainer::Table,
                        attributes: Attributes::new(),
                    })
                    .await
                }
                Event::End(TagEnd::Table) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::TableBody,
                    })
                    .await;
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Table,
                    })
                    .await;
                }
                Event::Start(Tag::TableHead) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::TableHead,
                        attributes: Attributes::new(),
                    })
                    .await;
                    co.yield_(IrEvent::Start {
                        container: IrContainer::TableRow,
                        attributes: Attributes::new(),
                    })
                    .await;

                    ctx.table_head_or_body = TableHeadOrBody::Head;
                }
                Event::End(TagEnd::TableHead) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::TableRow,
                    })
                    .await;
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::TableHead,
                    })
                    .await;
                    co.yield_(IrEvent::Start {
                        container: IrContainer::TableBody,
                        attributes: Attributes::new(),
                    })
                    .await;

                    ctx.table_head_or_body = TableHeadOrBody::Body;
                }
                Event::Start(Tag::TableRow) => {
                    ctx.table_cell_idx = 0;

                    co.yield_(IrEvent::Start {
                        container: IrContainer::TableRow,
                        attributes: Attributes::new(),
                    })
                    .await
                }
                Event::End(TagEnd::TableRow) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::TableRow,
                    })
                    .await;
                }
                Event::Start(Tag::TableCell) => {
                    let alignment = ctx
                        .table_alignment
                        .get(ctx.table_cell_idx)
                        .unwrap_or(&pulldown_cmark::Alignment::None);

                    let alignment = match alignment {
                        pulldown_cmark::Alignment::None => IrAlignment::Unspecified,
                        pulldown_cmark::Alignment::Left => IrAlignment::Left,
                        pulldown_cmark::Alignment::Center => IrAlignment::Center,
                        pulldown_cmark::Alignment::Right => IrAlignment::Right,
                    };
                    let head = matches!(ctx.table_head_or_body, TableHeadOrBody::Head);

                    co.yield_(IrEvent::Start {
                        container: IrContainer::TableCell { alignment, head },
                        attributes: Attributes::new(),
                    })
                    .await;
                }
                Event::End(TagEnd::TableCell) => {
                    let head = matches!(ctx.table_head_or_body, TableHeadOrBody::Head);

                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::TableCell { head },
                    })
                    .await;
                    ctx.table_cell_idx += 1;
                }

                Event::Start(Tag::Link {
                    dest_url,
                    title,
                    link_type: _,
                    id: _,
                }) => {
                    let mut attributes = Attributes::new();
                    if title.as_ref() != "" {
                        attributes.insert("title", Cow::from(title));
                    }
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Link {
                            destination: dest_url.into(),
                        },
                        attributes,
                    })
                    .await
                }
                Event::End(TagEnd::Link) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Link,
                    })
                    .await
                }

                Event::Start(Tag::Image {
                    dest_url,
                    title,
                    link_type: _,
                    id: _,
                }) => {
                    let alt = render_to_raw_string(iter_container_from_inside(&mut markdown));

                    let mut attributes = Attributes::new();
                    if title.as_ref() != "" {
                        attributes.insert("title", Cow::from(title));
                    }
                    co.yield_(IrEvent::Image {
                        destination: dest_url.into(),
                        alt,
                        attributes,
                    })
                    .await
                }
                Event::End(TagEnd::Image) => {
                    unreachable!()
                }

                Event::Start(Tag::Emphasis) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "em".into() },
                        attributes: Attributes::new(),
                    })
                    .await
                }
                Event::End(TagEnd::Emphasis) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "em".into() },
                    })
                    .await;
                }
                Event::Start(Tag::Strong) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "strong".into() },
                        attributes: Attributes::new(),
                    })
                    .await
                }
                Event::End(TagEnd::Strong) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "strong".into() },
                    })
                    .await;
                }
                Event::Start(Tag::Strikethrough) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "s".into() },
                        attributes: Attributes::new(),
                    })
                    .await
                }
                Event::End(TagEnd::Strikethrough) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "s".into() },
                    })
                    .await;
                }

                Event::Start(Tag::FootnoteDefinition(label)) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Footnote { label: label.into() },
                        attributes: Attributes::new(),
                    })
                    .await
                }
                Event::End(TagEnd::FootnoteDefinition) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Footnote,
                    })
                    .await
                }

                Event::Text(str) => co.yield_(IrEvent::Str(str.into())).await,
                Event::Code(str) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "code".into() },
                        attributes: Attributes::new(),
                    })
                    .await;
                    co.yield_(IrEvent::Str(str.into())).await;
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "code".into() },
                    })
                    .await;
                }
                Event::Html(html) | Event::InlineHtml(html) => {
                    co.yield_(IrEvent::HtmlInline {
                        content: html.into(),
                        attributes: Attributes::new(),
                    })
                    .await
                }

                Event::SoftBreak => co.yield_(IrEvent::Str("\n".into())).await,
                Event::HardBreak => {
                    co.yield_(IrEvent::HtmlInline {
                        content: "<br />".into(),
                        attributes: Attributes::new(),
                    })
                    .await
                }
                Event::Rule => {
                    co.yield_(IrEvent::HtmlInline {
                        content: "<hr />".into(),
                        attributes: Attributes::new(),
                    })
                    .await
                }

                Event::FootnoteReference(reference) => {
                    co.yield_(IrEvent::FootnoteReference {
                        reference: reference.into(),
                    })
                    .await
                }

                Event::TaskListMarker(checked) => co.yield_(IrEvent::TaskListMarker { checked }).await,
            }
        }

        for _ in ctx.section_stack {
            co.yield_(IrEvent::End {
                container: IrContainerEnd::Section,
            })
            .await;
        }
    })
    .into_iter()
}

/// Parse a markdown document to our intermediate markup representation.
pub fn parse<'s>(input: &'s str) -> impl Iterator<Item = IrEvent<'s>> {
    use pulldown_cmark::{Options, Parser};
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_HEADING_ATTRIBUTES;
    let p = Parser::new_ext(input, opts);
    markdown_to_ir(p)
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::parse;
    use crate::ir_markup;

    fn test(input: &str, output: &str) {
        let mut s = String::new();
        let ir = parse(input);
        ir_markup::push_html(&mut s, ir, &HashMap::new()).unwrap();
        assert_eq!(s, output);
    }

    #[test]
    fn paragraph() {
        test(
            r##"
This is
a simple paragraph

This is a new paragraph
                "##,
            r##"<p>This is
a simple paragraph</p>
<p>This is a new paragraph</p>
"##,
        );
    }

    #[test]
    fn blockquote() {
        test(
            r##"
A paragraph

> And a
> blockquote
                "##,
            r##"<p>A paragraph</p>
<blockquote>
<p>And a
blockquote</p>
</blockquote>"##,
        );
    }

    #[test]
    fn code() {
        test(
            r##"
```
plaintext <code />
```

```rust
enum A<T> {}
```
                "##,
            r##"<pre>
<code>
plaintext &lt;code /&gt;
</code>
</pre>
<pre class="highlight">
<code data-lang="rust">
<span class="keyword">enum</span> <span class="type">A</span><span class="punctuation">&lt;</span><span class="type">T</span><span class="punctuation">&gt;</span> <span class="punctuation">{</span><span class="punctuation">}</span>
</code>
</pre>"##,
        );
    }

    #[test]
    fn task_list() {
        test(
            r##"
- [ ] unchecked
- [x] checked
"##,
            r##"<ul>
<li>
<input type="checkbox" disabled="">
unchecked</li>
<li>
<input type="checkbox" disabled="" checked="">
checked</li>
</ul>
"##,
        )
    }
}
