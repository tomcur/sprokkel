use std::{cell::RefCell, sync::OnceLock};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter as Highlighter_};

type Configurations = dyn (Fn(&str) -> Option<&'static HighlightConfiguration>) + Send + Sync;
static CONFIGURATIONS: OnceLock<Box<Configurations>> = OnceLock::new();

thread_local!(static HIGHLIGHTER: RefCell<Highlighter> = RefCell::new(Highlighter::new()));

/// Tuple of (treesitter higlight, neovim highlight group)
// perhaps Neovim treesitter highlight groups can be used directly (but all would have to be linked
// in the config file)
static HIGHLIGHT_NAMES: &[&str] = &[
    ("attribute"),
    ("constant"),
    ("comment"),
    ("function.builtin"),
    ("function"),
    ("keyword"),
    ("operator"),
    ("property"),
    ("punctuation"),
    ("string"),
    ("string.special"),
    ("tag"),
    ("type"),
    ("variable"),
];

pub enum Error {
    InvalidLanguage,
    Other,
}

impl From<tree_sitter_highlight::Error> for Error {
    fn from(value: tree_sitter_highlight::Error) -> Self {
        match value {
            tree_sitter_highlight::Error::InvalidLanguage => Error::InvalidLanguage,
            _ => Error::Other,
        }
    }
}

struct Highlighter {
    highlighter: Highlighter_,
    configurations: &'static Configurations,
}

// struct Configurations(Box<dyn (Fn(&str) -> Option<&'static HighlightConfiguration>) + Send + Sync>);
//
fn init_configurations() -> Box<Configurations> {
    let bash_config = Box::leak::<'static>(Box::new({
        let highlights = tree_sitter_bash::HIGHLIGHT_QUERY;
        HighlightConfiguration::new(tree_sitter_bash::language(), &highlights, "", "").unwrap()
    }));
    bash_config.configure(HIGHLIGHT_NAMES);

    let c_config = Box::leak::<'static>(Box::new({
        let highlights = tree_sitter_c::HIGHLIGHT_QUERY;
        HighlightConfiguration::new(tree_sitter_c::language(), &highlights, "", "").unwrap()
    }));
    c_config.configure(HIGHLIGHT_NAMES);

    let cpp_config = Box::leak::<'static>(Box::new({
        let highlights = tree_sitter_cpp::HIGHLIGHT_QUERY;
        HighlightConfiguration::new(tree_sitter_cpp::language(), &highlights, "", "").unwrap()
    }));
    cpp_config.configure(HIGHLIGHT_NAMES);

    let djot_config = Box::leak::<'static>(Box::new({
        let highlights = tree_sitter_djot::HIGHLIGHTS_QUERY;
        HighlightConfiguration::new(
            tree_sitter_djot::language(),
            &highlights,
            tree_sitter_djot::INJECTIONS_QUERY,
            "",
        )
        .unwrap()
    }));
    djot_config.configure(HIGHLIGHT_NAMES);

    let nix_config = Box::leak::<'static>(Box::new({
        let highlights = tree_sitter_nix::HIGHLIGHTS_QUERY;
        HighlightConfiguration::new(tree_sitter_nix::language(), &highlights, "", "").unwrap()
    }));
    nix_config.configure(HIGHLIGHT_NAMES);

    let python_config = Box::leak::<'static>(Box::new({
        let highlights = tree_sitter_python::HIGHLIGHT_QUERY;
        HighlightConfiguration::new(tree_sitter_python::language(), &highlights, "", "").unwrap()
    }));
    nix_config.configure(HIGHLIGHT_NAMES);

    let rust_config = Box::leak::<'static>(Box::new({
        let highlights = tree_sitter_rust::HIGHLIGHT_QUERY;
        HighlightConfiguration::new(
            tree_sitter_rust::language(),
            &highlights,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        )
        .unwrap()
    }));
    rust_config.configure(HIGHLIGHT_NAMES);

    let toml_config = Box::leak::<'static>(Box::new({
        let highlights = tree_sitter_toml::HIGHLIGHT_QUERY;
        HighlightConfiguration::new(tree_sitter_toml::language(), &highlights, "", "").unwrap()
    }));
    toml_config.configure(HIGHLIGHT_NAMES);

    let typescript_config = Box::leak::<'static>(Box::new({
        let highlights: String = [
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_typescript::HIGHLIGHT_QUERY,
        ]
        .into_iter()
        .collect();
        let locals: String = [
            tree_sitter_javascript::LOCALS_QUERY,
            tree_sitter_typescript::LOCALS_QUERY,
        ]
        .into_iter()
        .collect();
        HighlightConfiguration::new(
            tree_sitter_typescript::language_typescript(),
            &highlights,
            "",
            &locals,
        )
        .unwrap()
    }));
    typescript_config.configure(HIGHLIGHT_NAMES);

    let highlight_configurations = |language: &'_ str| match language {
        "bash" | "sh" | "shell" => Some(bash_config as &'static _),
        "c" => Some(c_config as &'static _),
        "cpp" | "c++" => Some(cpp_config as &'static _),
        "djot" => Some(djot_config as &'static _),
        "nix" => Some(nix_config as &'static _),
        "python" => Some(python_config as &'static _),
        "rust" => Some(rust_config as &'static _),
        "toml" => Some(toml_config as &'static _),
        "typescript" | "ts" | "javascript" | "js" => Some(typescript_config as &'static _),
        _ => None,
    };

    Box::new(highlight_configurations)
}

impl Highlighter {
    pub fn new() -> Self {
        let configurations = CONFIGURATIONS.get_or_init(init_configurations);

        Highlighter {
            highlighter: Highlighter_::new(),
            configurations,
        }
    }
}

pub fn highlight(code: &str, language: &str) -> Result<String, Error> {
    let code = code.as_bytes();

    HIGHLIGHTER.with_borrow_mut(|this| {
        let config = (*this.configurations)(language)
            .ok_or(tree_sitter_highlight::Error::InvalidLanguage)?;
        let highlights = this
            .highlighter
            .highlight(config, code, None, |lang| (*this.configurations)(lang))?;

        let mut buf = Vec::with_capacity(code.len());
        for event in highlights {
            let event = event?;
            match event {
                HighlightEvent::Source { start, end } => {
                    for &char in code[start..end].iter() {
                        match tree_sitter_highlight::util::html_escape(char) {
                            Some(esc) => buf.extend_from_slice(esc),
                            None => buf.extend_from_slice(&[char]),
                        };
                    }
                }
                HighlightEvent::HighlightStart(s) => {
                    let mut class = std::borrow::Cow::Borrowed(HIGHLIGHT_NAMES[s.0]);
                    if class.contains('.') {
                        class = std::borrow::Cow::Owned(class.replace('.', " "));
                    }
                    buf.extend_from_slice(format!(r#"<span class="{class}">"#).as_bytes());
                }
                HighlightEvent::HighlightEnd => {
                    buf.extend_from_slice("</span>".as_bytes());
                }
            }
        }

        Ok(String::from_utf8(buf).expect("valid Unicode"))
    })
}
