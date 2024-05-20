#![cfg(test)]

mod djot_markdown_equal {
    use std::collections::HashMap;

    use crate::{djot, ir_markup, markdown};

    fn djot_markdown_equal(djot: &str, markdown: &str) {
        let djot = djot::parse(djot);
        let markdown = markdown::parse(markdown);

        let mut dhtml = String::new();
        ir_markup::push_html(&mut dhtml, djot, &HashMap::new()).unwrap();

        let mut mhtml = String::new();
        ir_markup::push_html(&mut mhtml, markdown, &HashMap::new()).unwrap();

        assert_eq!(dhtml, mhtml);
    }

    #[test]
    fn empty() {
        djot_markdown_equal("", "");
    }

    #[test]
    fn table() {
        djot_markdown_equal(
            r#"
| head 1 | head 2 |
|--|--:|
| cell 1 | cell 2 |
"#,
            r#"
| head 1 | head 2 |
|---|--:|
| cell 1 | cell 2 |
"#,
        );
    }

    #[test]
    fn doc() {
        djot_markdown_equal(
            r#"
# A document

With a paragraph
[containing a link](https://example.com)

<an-email-address@example.com>

![alt text](https://example.com/image.svg)

- a
- list

Some features:

1. ordinal list
1. _emphasis_
1. *bold*
1. "smart punctuation"
1. 'other smart punctuation'

## And a subheading

...
"#,
            r#"
# A document { #A-document }

With a paragraph
[containing a link](https://example.com)

<an-email-address@example.com>

![alt text](https://example.com/image.svg)

- a
- list

Some features:

1. ordinal list
1. *emphasis*
1. **bold**
1. "smart punctuation"
1. 'other smart punctuation'

## And a subheading { #And-a-subheading }

...
"#,
        );
    }
}
