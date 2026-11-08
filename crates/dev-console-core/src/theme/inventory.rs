//! Re-parses the shipped stylesheets at test time so the generated Rust tables can be
//! checked against them.
//!
//! `src/styles/tokens.css` and `src/styles/omarchy.css` are what the Svelte frontend
//! actually applies, so they are the source of truth. [`crate::theme::shared`] and
//! [`crate::theme::palettes`] hold a generated copy. This module reads the originals with
//! a deliberately small CSS block scanner - selectors, declarations, `@media` nesting,
//! nothing else - because a parser that understands less is a parser that cannot quietly
//! agree with a wrong table.

/// A declared palette: its name, the `color-scheme` it declares, and its `--token` rows in
/// stylesheet order.
#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    pub scheme: String,
    pub rows: Vec<(String, String)>,
}

struct Block {
    selector: String,
    body: String,
    parent: Option<String>,
}

/// The path of the stylesheet the Svelte app loads.
pub fn tokens_path() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/styles/tokens.css")
}

pub fn omarchy_path() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/styles/omarchy.css")
}

pub fn read(path: &str) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {path} (the stylesheet moved?): {error}"))
}

/// The shared `:root` block: every non-colour token, in declaration order.
pub fn shared_rows(css: &str) -> Vec<(String, String)> {
    let block = top_level_block(css, ":root").expect("tokens.css has no shared `:root` block");
    tokens(&block)
}

/// Every palette: `dark`, `light`, then the Omarchy palettes in file order - the order the
/// registry must match.
pub fn palettes(tokens_css: &str, omarchy_css: &str) -> Vec<Sheet> {
    let dark = top_level_block(tokens_css, ":root, :root[data-theme='dark']")
        .expect("tokens.css has no dark palette block");
    let light = top_level_block(tokens_css, ":root[data-theme='light']")
        .expect("tokens.css has no light palette block");
    let shared_scheme = color_scheme(&declarations(
        &top_level_block(tokens_css, ":root").expect("tokens.css has no shared `:root` block"),
    ));
    let dark_scheme = color_scheme(&declarations(&dark)).or_else(|| shared_scheme.clone());

    let mut sheets = vec![
        sheet(
            "dark",
            &dark_scheme.expect("no color-scheme for the dark palette"),
            &dark,
        ),
        sheet(
            "light",
            &color_scheme(&declarations(&light))
                .expect("the light palette declares no color-scheme"),
            &light,
        ),
    ];

    for block in blocks(omarchy_css) {
        if block.parent.is_some() || !block.selector.starts_with(":root[data-theme='omarchy-") {
            continue;
        }
        let name = block
            .selector
            .split('\'')
            .nth(1)
            .unwrap_or_else(|| panic!("unreadable palette selector {}", block.selector))
            .to_string();
        let scheme = color_scheme(&declarations(&block))
            .unwrap_or_else(|| panic!("the {name} palette declares no color-scheme"));
        sheets.push(sheet(&name, &scheme, &block));
    }
    sheets
}

/// The block inside `@media (prefers-color-scheme: light)`, which must stay identical to
/// the explicit `[data-theme='light']` one.
pub fn system_light(tokens_css: &str) -> Vec<(String, String)> {
    blocks(tokens_css)
        .into_iter()
        .find(|block| {
            block.selector == ":root"
                && block
                    .parent
                    .as_deref()
                    .is_some_and(|parent| parent.starts_with("@media (prefers-color-scheme:"))
        })
        .map(|block| tokens(&block))
        .expect("tokens.css has no :root rule inside the prefers-color-scheme light block")
}

fn sheet(name: &str, scheme: &str, block: &Block) -> Sheet {
    Sheet {
        name: name.to_string(),
        scheme: scheme.to_string(),
        rows: tokens(block),
    }
}

fn tokens(block: &Block) -> Vec<(String, String)> {
    declarations(block)
        .into_iter()
        .filter(|(name, _)| name.starts_with("--"))
        .collect()
}

fn color_scheme(declarations: &[(String, String)]) -> Option<String> {
    declarations
        .iter()
        .find(|(name, _)| name == "color-scheme")
        .map(|(_, value)| value.clone())
}

fn declarations(block: &Block) -> Vec<(String, String)> {
    block
        .body
        .split(';')
        .filter_map(|entry| {
            let (name, value) = entry.split_once(':')?;
            let name = name.trim().to_string();
            if name.is_empty() || value.trim().is_empty() {
                return None;
            }
            Some((name, value.trim().to_string()))
        })
        .collect()
}

fn top_level_block(css: &str, selector: &str) -> Option<Block> {
    blocks(css)
        .into_iter()
        .find(|block| block.parent.is_none() && block.selector == selector)
}

/// Every `selector { body }` pair, with the selector of the block each one sits in.
fn blocks(css: &str) -> Vec<Block> {
    let css = strip_comments(css);
    let mut out = Vec::new();
    collect(&css, None, &mut out);
    out
}

fn collect(css: &str, parent: Option<&str>, out: &mut Vec<Block>) {
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = matching_brace(rest, open) else {
            return;
        };
        let selector = normalize(&rest[..open]);
        let body = &rest[open + 1..close];
        if !selector.is_empty() {
            out.push(Block {
                selector: selector.clone(),
                body: without_nested_blocks(body).trim().to_string(),
                parent: parent.map(str::to_string),
            });
        }
        collect(body, Some(&selector), out);
        rest = &rest[close + 1..];
    }
}

fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, char) in text.char_indices().skip_while(|(index, _)| *index < open) {
        match char {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn without_nested_blocks(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut depth = 0usize;
    for char in body.chars() {
        match char {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(char),
            _ => {}
        }
    }
    out
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        let end = rest[at..]
            .find("*/")
            .map(|at| at + 2)
            .unwrap_or(rest.len() - at);
        out.push_str(&rest[..at]);
        out.extend(rest[at..at + end].chars().filter(|char| *char == '\n'));
        rest = &rest[at + end..];
    }
    out.push_str(rest);
    out
}

fn normalize(selector: &str) -> String {
    let collapsed = selector.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim().to_string()
}
