Sprokkel is a lightweight static site generator. It supports:

- writing in [Djot](https://github.com/jgm/djot) and [CommonMark-compliant
  Markdown](https://commonmark.org) markup languages
- [Tree-sitter](https://tree-sitter.github.io/tree-sitter)-based code syntax
  highlighting
- compile-time LaTeX to to MathML rendering
- image resizing and re-encoding
- templating using [minijinja](https://github.com/mitsuhiko/minijinja)
- pagination
- static assets
- back-references: i.e., "which entries link here?"

Sprokkel does not currently implement shortcodes and/or scripting.

The goal is to provide some useful abstractions, especially related to file
paths, while remaining relatively magic-free.

## Alternatives

- Sprokkel does not aim for complete flexibility. You may want to use a static
  site generator written in a language with runtime evaluation, like
  [Bagatto](https://bagatto.co/), instead.

- Sprokkel does not come with a ton of batteries included. You may want to use a
  static site generator like [Zola](https://www.getzola.org) or
  [Hugo](https://gohugo.io) instead.
