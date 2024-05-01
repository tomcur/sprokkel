# Sprokkel /'sprɔkəl/ 🕊️

A lightweight static site generator. For example, see
<https://sprokkel.uint.one>.

It supports:

- writing in [Djot markup](https://github.com/jgm/djot)
- [Tree-sitter](https://github.com/tree-sitter/tree-sitter)-based code syntax
  highlighting
- compile-time LaTeX to to MathML rendering
- image resizing and re-encoding
- templating using [minijinja](https://github.com/mitsuhiko/minijinja)
- pagination
- static assets
- back-references: i.e., "which entries link here?"

## Usage

Build a site for release:

```bash
$ sprokkel build
```

Build every time the site changes:

```bash
$ sprokkel build --watch
```

## Installation

Using Cargo

```bash
$ cargo install sprokkel

# With katex-based LaTeX to MathML rendering
$ cargo install sprokkel --features katex
```

using Nix

## Functionality

Sprokkel is built around entries and templates. Entries are written in Djot
markup. Templates tell Sprokkel how to build full HTML pages. Every entry
belongs to a group, and every group can have its own template.

### ./entries

Entries are Djot files (.dj), and all entries belong to a group. Entries are
rendered to HTML using their group's template. An entry's group is determined
by the directory the entry is in. For example, `./entries/blog/foo.dj` belongs
to the "foo" group.

Entries must be nested directly under their group's directory or in their own
directory. For example:

```
./entries
├── blog
│   ├── 2024-03-12-something
│   │   ├── index.dj
│   │   └── image.svg
│   └── 2024-04-24-something-else.dj
└── projects
    └── foobar.dj
```


### ./templates

All templates are in `./templates`. Entry group templates should be named after
their group as `_<group>.html`. For example, entry `./entries/blog/foo.dj` will
be rendered using `./templates/_blog.html` as template. If the template for a
group is missing, Sprokkel falls back to the `./templates/_entry.html`
template. This template must exist.

Templates where no part of the file path starts without an underscore are
rendered as pages, preserving the directory structure. For example,
`./templates/foo/index.html` is rendered to `./out/foo/index.html`, but
`./templates/foo/_bar/baz.html` is not rendered directly.

### Assets

Sprokkel supports two types of asset.

### ./assets

Static assets must be placed in `./assets`. These are copied as-is to the
output directory, preserving the directory structure. For example, a file
`./assets/foo/bar/baz.qux` is copied to `./out/foo/bar/baz.qux`.

#### ./cat 🐈‍⬛

_Cat_ assets are placed in `./cat`. These are concatenated to a single output
file per directory and optionally preprocessed. The directory structure of
`cat` must be a tree where only leaf nodes contain files. For example:

```
./cat
├── css
│   └── style.css
│       ├── 00_fonts.css
│       ├── 01_highlight.css
│       └── 02_main.css
└── js
    └── main.js
        ├── 00_colorscheme_toggle.js
        └── 01_email_obfuscator.js
```

This produces two files: `./out/css/style.css` and `./out/js/main.js`.
