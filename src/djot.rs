//! Transform jotdown events into our intermediate markup representation.

use jotdown::{Alignment, Attributes, Container, Event, ListKind, OrderedListNumbering};
use std::borrow::Cow;

use crate::ir_markup::{
    Alignment as IrAlignment, Container as IrContainer, ContainerEnd as IrContainerEnd, Event as IrEvent,
    ListKind as IrListKind, MathKind as IrMathKind, OrderedListNumbering as IrOrderedListNumbering,
};

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
fn iter_container_from_inside<'a>(iter: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    iter_container(
        // prepend a fake start block
        [Event::Start(Container::Paragraph, Attributes::new())]
            .into_iter()
            .chain(iter),
    )
}

fn render_to_raw_string<'a>(events: impl Iterator<Item = Event<'a>>) -> Cow<'a, str> {
    let mut string: Cow<'a, str> = "".into();

    for event in events {
        match event {
            Event::Str(val) => {
                if string.as_ref() == "" {
                    string = val;
                } else {
                    string.to_mut().push_str(val.as_ref())
                }
            }
            Event::Ellipsis => string.to_mut().push('…'),
            Event::EnDash => string.to_mut().push('–'),
            Event::EmDash => string.to_mut().push('—'),
            Event::NonBreakingSpace => string.to_mut().push(' '),
            Event::Symbol(val) => {
                string.to_mut().push(':');
                string.to_mut().push_str(val.as_ref());
                string.to_mut().push(':')
            }
            _ => {}
        }
    }

    string
}

impl From<ListKind> for IrListKind {
    fn from(value: ListKind) -> Self {
        match value {
            ListKind::Unordered => IrListKind::Unordered,
            ListKind::Ordered {
                numbering,
                style: _,
                start,
            } => IrListKind::Ordered {
                numbering: match numbering {
                    OrderedListNumbering::Decimal => IrOrderedListNumbering::Decimal,
                    OrderedListNumbering::RomanLower => IrOrderedListNumbering::RomanLower,
                    OrderedListNumbering::RomanUpper => IrOrderedListNumbering::RomanUpper,
                    OrderedListNumbering::AlphaLower => IrOrderedListNumbering::AlphaLower,
                    OrderedListNumbering::AlphaUpper => IrOrderedListNumbering::AlphaUpper,
                },
                start,
            },
            ListKind::Task => IrListKind::Task,
        }
    }
}

impl From<Alignment> for IrAlignment {
    fn from(value: Alignment) -> Self {
        match value {
            Alignment::Unspecified => IrAlignment::Unspecified,
            Alignment::Left => IrAlignment::Left,
            Alignment::Center => IrAlignment::Center,
            Alignment::Right => IrAlignment::Right,
        }
    }
}

pub fn djot_to_ir<'s>(mut djot: impl Iterator<Item = Event<'s>>) -> impl Iterator<Item = IrEvent<'s>> {
    // to be replaced by `gen`-blocks
    genawaiter::rc::Gen::new(|co| async move {
        while let Some(ev) = djot.next() {
            match ev {
                Event::Start(Container::Blockquote, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Blockquote,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Blockquote) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Blockquote,
                    })
                    .await
                }

                Event::Start(Container::DescriptionList, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::DescriptionList,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::DescriptionList) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::DescriptionList,
                    })
                    .await
                }
                Event::Start(Container::DescriptionTerm, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::DescriptionTerm,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::DescriptionTerm) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::DescriptionTerm,
                    })
                    .await
                }
                Event::Start(Container::DescriptionDetails, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::DescriptionDetails,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::DescriptionDetails) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::DescriptionDetails,
                    })
                    .await
                }

                Event::Start(
                    Container::Heading {
                        level,
                        has_section: _,
                        id,
                    },
                    attributes,
                ) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Heading { level, id },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Heading {
                    level,
                    has_section: _,
                    id: _,
                }) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Heading { level },
                    })
                    .await
                }
                Event::Start(Container::Section { id }, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Section { id },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Section { id: _ }) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Section,
                    })
                    .await
                }
                Event::Start(Container::Div { class: _ }, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Div,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Div { class: _ }) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Div,
                    })
                    .await
                }
                Event::Start(Container::Paragraph, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Paragraph,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Paragraph) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Paragraph,
                    })
                    .await
                }
                Event::Start(Container::Image(destination, _link_type), attributes) => {
                    let alt = render_to_raw_string(iter_container_from_inside(&mut djot));
                    co.yield_(IrEvent::Image {
                        destination,
                        alt,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Image(_, _)) => unreachable!(),
                Event::Start(Container::Link(destination, _link_type), attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Link { destination },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Link(_, _)) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Link,
                    })
                    .await
                }

                Event::Start(Container::CodeBlock { language }, attributes) => {
                    let code = render_to_raw_string(iter_container_from_inside(&mut djot));
                    co.yield_(IrEvent::CodeBlock {
                        language: language.into(),
                        code,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::CodeBlock { .. }) => unreachable!(),
                Event::Start(Container::Math { display }, attributes) => {
                    let math = render_to_raw_string(iter_container_from_inside(&mut djot));
                    co.yield_(IrEvent::Math {
                        kind: if display {
                            IrMathKind::Display
                        } else {
                            IrMathKind::Inline
                        },
                        math,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Math { .. }) => unreachable!(),

                Event::Start(Container::RawInline { format }, attributes) => {
                    let content = render_to_raw_string(iter_container_from_inside(&mut djot));
                    if matches!(format, "html" | "HTML") {
                        co.yield_(IrEvent::HtmlInline { content, attributes }).await
                    }
                }
                Event::End(Container::RawInline { .. }) => unreachable!(),
                Event::Start(Container::RawBlock { format }, attributes) => {
                    let content = render_to_raw_string(iter_container_from_inside(&mut djot));
                    if matches!(format, "html" | "HTML") {
                        co.yield_(IrEvent::HtmlBlock { content, attributes }).await
                    }
                }
                Event::End(Container::RawBlock { .. }) => unreachable!(),

                Event::Start(Container::List { kind, tight }, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::List {
                            kind: kind.into(),
                            tight,
                        },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::List { kind, tight: _ }) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::List { kind: kind.into() },
                    })
                    .await
                }
                Event::Start(Container::ListItem, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::ListItem,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::ListItem) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::ListItem,
                    })
                    .await
                }
                Event::Start(Container::TaskListItem { checked }, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::TaskListItem { checked },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::TaskListItem { checked: _ }) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::TaskListItem,
                    })
                    .await
                }

                Event::Start(Container::Table, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Table,
                        attributes,
                    })
                    .await;
                    co.yield_(IrEvent::Start {
                        container: IrContainer::TableBody,
                        attributes: jotdown::Attributes::new(),
                    })
                    .await;
                }
                Event::End(Container::Table) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::TableBody,
                    })
                    .await;
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Table,
                    })
                    .await;
                }
                Event::Start(Container::TableRow { head: _ }, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::TableRow,
                        attributes,
                    })
                    .await
                }
                Event::End(Container::TableRow { head: _ }) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::TableRow,
                    })
                    .await
                }
                Event::Start(Container::TableCell { alignment, head }, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::TableCell {
                            alignment: alignment.into(),
                            head,
                        },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::TableCell { alignment: _, head }) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::TableCell { head },
                    })
                    .await
                }

                Event::Start(Container::Footnote { label }, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Footnote { label: label.into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Footnote { label: _ }) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Footnote,
                    })
                    .await
                }

                Event::Start(Container::LinkDefinition { label: _ }, _attributes) => {}
                Event::End(Container::LinkDefinition { label: _ }) => {}

                Event::Start(Container::Caption, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "caption".into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Caption) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "caption".into() },
                    })
                    .await
                }
                Event::Start(Container::Verbatim, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "code".into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Verbatim) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "code".into() },
                    })
                    .await
                }
                Event::Start(Container::Span, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "span".into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Span) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "span".into() },
                    })
                    .await
                }
                Event::Start(Container::Subscript, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "sub".into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Subscript) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "sub".into() },
                    })
                    .await
                }
                Event::Start(Container::Superscript, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "sup".into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Superscript) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "sup".into() },
                    })
                    .await
                }
                Event::Start(Container::Insert, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "ins".into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Insert) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "ins".into() },
                    })
                    .await
                }
                Event::Start(Container::Delete, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "del".into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Delete) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "del".into() },
                    })
                    .await
                }
                Event::Start(Container::Strong, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "strong".into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Strong) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "strong".into() },
                    })
                    .await
                }
                Event::Start(Container::Emphasis, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "em".into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Emphasis) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "em".into() },
                    })
                    .await
                }
                Event::Start(Container::Mark, attributes) => {
                    co.yield_(IrEvent::Start {
                        container: IrContainer::Other { tag: "mark".into() },
                        attributes,
                    })
                    .await
                }
                Event::End(Container::Mark) => {
                    co.yield_(IrEvent::End {
                        container: IrContainerEnd::Other { tag: "mark".into() },
                    })
                    .await
                }

                Event::Str(str) | Event::Symbol(str) => co.yield_(IrEvent::Str(str)).await,

                Event::Softbreak => co.yield_(IrEvent::Str("\n".into())).await,
                Event::Hardbreak => {
                    co.yield_(IrEvent::HtmlInline {
                        content: "<br />".into(),
                        attributes: jotdown::Attributes::new(),
                    })
                    .await
                }
                Event::NonBreakingSpace => {
                    co.yield_(IrEvent::HtmlInline {
                        content: "&nbsp;".into(),
                        attributes: jotdown::Attributes::new(),
                    })
                    .await
                }
                Event::Escape | Event::Blankline => {}

                Event::FootnoteReference(reference) => {
                    co.yield_(IrEvent::FootnoteReference {
                        reference: reference.into(),
                    })
                    .await
                }

                Event::ThematicBreak(attributes) => {
                    co.yield_(IrEvent::TagWithAttribute {
                        tag: "hr".into(),
                        attributes,
                    })
                    .await
                }

                Event::Ellipsis => co.yield_(IrEvent::Str("…".into())).await,
                Event::EmDash => co.yield_(IrEvent::Str("–".into())).await,
                Event::EnDash => co.yield_(IrEvent::Str("—".into())).await,
                Event::LeftSingleQuote => co.yield_(IrEvent::Str("‘".into())).await,
                Event::LeftDoubleQuote => co.yield_(IrEvent::Str("“".into())).await,
                Event::RightSingleQuote => co.yield_(IrEvent::Str("’".into())).await,
                Event::RightDoubleQuote => co.yield_(IrEvent::Str("”".into())).await,
            }
        }
    })
    .into_iter()
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use jotdown::Parser;

    use super::djot_to_ir;
    use crate::ir_markup;

    fn test(input: &str, output: &str) {
        let mut s = String::new();
        let p = Parser::new(input);
        let ir = djot_to_ir(p);
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
    fn heading() {
        test(
            r##"
# This is a heading test

## With a nested heading

The following heading is equal to a previous one

# This is a heading test
"##,
            r##"<section id="This-is-a-heading-test">
<h1><a href="#This-is-a-heading-test">This is a heading test</a></h1>
<section id="With-a-nested-heading">
<h2><a href="#With-a-nested-heading">With a nested heading</a></h2>
<p>The following heading is equal to a previous one</p>
</section>
</section>
<section id="This-is-a-heading-test-1">
<h1><a href="#This-is-a-heading-test-1">This is a heading test</a></h1>
</section>
"##,
        )
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
    fn list() {
        test(
            r##"
- item 1
- item 2

  1. a nested item
"##,
            r##"<ul>
<li>item 1</li>
<li>item 2
<ol>
<li>a nested item</li>
</ol>
</li>
</ul>
"##,
        )
    }

    #[test]
    fn ordered_list() {
        test(
            r##"
1. item 1
1. item 2

{}
3. item 1
3. item 2

{}
a. item 1
a. item 2

{}
I. item 1
I. item 2
"##,
            r##"<ol>
<li>item 1</li>
<li>item 2</li>
</ol>
<ol start="3">
<li>item 1</li>
<li>item 2</li>
</ol>
<ol type="a">
<li>item 1</li>
<li>item 2</li>
</ol>
<ol type="I">
<li>item 1</li>
<li>item 2</li>
</ol>
"##,
        )
    }

    #[test]
    fn description_list() {
        test(
            r##"
: term

  item

: term 2

  item 2
"##,
            r##"<dl>
<dt>term</dt>
<dd>
<p>item</p>
</dd>
<dt>term 2</dt>
<dd>
<p>item 2</p>
</dd>
</dl>
"##,
        )
    }

    #[test]
    fn task_list() {
        test(
            r##"
- [ ] unchecked
- [x] checked
"##,
            r##"<ul class="task-list">
<li class="unchecked" data-checked="false">unchecked</li>
<li class="checked" data-checked="true">checked</li>
</ul>
"##,
        )
    }

    #[test]
    fn characters() {
        test(
            r##"
'does this work?'

"how about this...?"
"##,
            r##"<p>‘does this work?’</p>
<p>“how about this…?”</p>
"##,
        )
    }

    #[test]
    fn table() {
        test(
            r##"
| head 1 | head 2 |
|--|--:|
| cell 1 | cell 2 |
"##,
            r##"<table>
<tbody>
<tr>
<th>head 1</th>
<th style="text-align: right;">head 2</th>
</tr>
<tr>
<td>cell 1</td>
<td style="text-align: right;">cell 2</td>
</tr>
</tbody>
</table>
"##,
        )
    }
}
