// File: file_refs.rs
//
// GUI file mention expansion.
// file_refs.rs
// ├── expand_file_references()  # @path mentions -> model content blocks
// ├── complete_file_reference() # @path autocomplete
// ├── find_file_mentions()      # token parser shared by expansion/tests
// └── render_* helpers          # bounded text and directory serialization

use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use base64::Engine;
use rozsa_model::types::ContentBlock;
use serde::Serialize;

const MAX_TEXT_LINES: usize = 2000;
const MAX_DIR_ENTRIES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMention {
    pub token: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct FileReferenceExpansion {
    pub blocks: Vec<ContentBlock>,
    pub display_text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutocompleteItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

pub fn expand_file_references(text: &str, cwd: &Path) -> FileReferenceExpansion {
    let mentions = find_file_mentions(text);
    if mentions.is_empty() {
        return FileReferenceExpansion {
            blocks: Vec::new(),
            display_text: None,
        };
    }

    let mut seen = HashSet::new();
    let mut blocks = Vec::new();
    for mention in mentions {
        let path = resolve_mention_path(cwd, &mention.path);
        if !seen.insert(path.clone()) || !path.exists() {
            continue;
        }

        if path.is_dir() {
            if let Some(text) = render_directory_block(&path) {
                blocks.push(text_block(text));
            }
            continue;
        }

        if let Some(mime_type) = supported_image_mime(&path) {
            if let Ok(bytes) = fs::read(&path) {
                blocks.push(text_block(format!(
                    "<file>\n<path>{}</path>\n[Image attached as model input]\n</file>",
                    path.display()
                )));
                blocks.push(ContentBlock::Image {
                    data: base64::engine::general_purpose::STANDARD.encode(bytes),
                    mime_type: mime_type.to_string(),
                });
            }
            continue;
        }

        if is_unsupported_binary(&path) {
            continue;
        }

        if let Some(text) = render_text_file_block(&path) {
            blocks.push(text_block(text));
        }
    }

    FileReferenceExpansion {
        display_text: if blocks.is_empty() {
            None
        } else {
            Some(text.to_string())
        },
        blocks,
    }
}

pub fn complete_file_reference(prefix: &str, cwd: &Path) -> Vec<AutocompleteItem> {
    let raw = prefix
        .strip_prefix("@\"")
        .or_else(|| prefix.strip_prefix('@'))
        .unwrap_or(prefix);
    let quoted = prefix.starts_with("@\"");
    let expanded = expand_home(raw);
    let (search_dir, name_prefix, display_dir) = split_completion_path(cwd, raw, &expanded);

    let Ok(entries) = fs::read_dir(&search_dir) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name
            .to_ascii_lowercase()
            .starts_with(&name_prefix.to_ascii_lowercase())
        {
            continue;
        }

        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let mut display_path = join_display_path(&display_dir, &name);
        if is_dir {
            display_path.push('/');
        }
        let value_path = if quoted || display_path.contains(' ') {
            format!("@\"{display_path}\"")
        } else {
            format!("@{display_path}")
        };
        let value = if is_dir {
            value_path
        } else {
            format!("{value_path} ")
        };
        items.push(AutocompleteItem {
            value,
            label: format!("{}{}", name, if is_dir { "/" } else { "" }),
            description: Some(display_path),
        });
    }

    items.sort_by(|a, b| {
        let a_dir = a.label.ends_with('/');
        let b_dir = b.label.ends_with('/');
        b_dir.cmp(&a_dir).then_with(|| a.label.cmp(&b.label))
    });
    items.truncate(50);
    items
}

pub fn find_file_mentions(text: &str) -> Vec<FileMention> {
    let chars: Vec<char> = text.chars().collect();
    let mut mentions = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '@' || (i > 0 && !chars[i - 1].is_whitespace()) {
            i += 1;
            continue;
        }

        if chars.get(i + 1) == Some(&'"') {
            let mut end = i + 2;
            while end < chars.len() && chars[end] != '"' {
                end += 1;
            }
            if end < chars.len() {
                let path: String = chars[i + 2..end].iter().collect();
                let token: String = chars[i..=end].iter().collect();
                if !path.is_empty() {
                    mentions.push(FileMention { token, path });
                }
                i = end + 1;
                continue;
            }
        }

        let mut end = i + 1;
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }
        let path: String = chars[i + 1..end].iter().collect();
        let token: String = chars[i..end].iter().collect();
        if !path.is_empty() {
            mentions.push(FileMention { token, path });
        }
        i = end;
    }
    mentions
}

fn render_text_file_block(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut out = String::new();
    let mut count = 0usize;
    let mut truncated = false;

    for line in reader.lines() {
        let line = line.ok()?;
        count += 1;
        if count > MAX_TEXT_LINES {
            truncated = true;
            break;
        }
        out.push_str(&format!("{count:>6}\t{line}\n"));
    }

    if out.is_empty() {
        return None;
    }
    if truncated {
        out.push_str(&format!(
            "\n[File truncated after {MAX_TEXT_LINES} lines. Use the read file tool for the rest.]\n"
        ));
    }

    Some(format!(
        "<file>\n<path>{}</path>\n<content>\n{}</content>\n</file>",
        path.display(),
        out
    ))
}

fn render_directory_block(path: &Path) -> Option<String> {
    let mut entries = fs::read_dir(path)
        .ok()?
        .flatten()
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let suffix = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                "/"
            } else {
                ""
            };
            format!("{name}{suffix}")
        })
        .collect::<Vec<_>>();
    entries.sort();
    let total = entries.len();
    entries.truncate(MAX_DIR_ENTRIES);
    let mut body = entries.join("\n");
    if total > MAX_DIR_ENTRIES {
        body.push_str(&format!(
            "\n[Directory truncated: showing {MAX_DIR_ENTRIES} of {total} entries.]"
        ));
    }
    Some(format!(
        "<file>\n<path>{}</path>\n<content type=\"directory\">\n{}\n</content>\n</file>",
        path.display(),
        body
    ))
}

fn resolve_mention_path(cwd: &Path, path: &str) -> PathBuf {
    let expanded = expand_home(path);
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

fn split_completion_path(cwd: &Path, raw: &str, expanded: &str) -> (PathBuf, String, String) {
    let raw_path = Path::new(raw);
    let expanded_path = Path::new(expanded);
    let ends_with_sep = raw.ends_with('/');
    let (dir_raw, name_prefix) = if ends_with_sep {
        (raw, "")
    } else {
        (
            raw_path
                .parent()
                .and_then(|p| p.to_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(""),
            raw_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        )
    };

    let search_dir = if ends_with_sep {
        if expanded_path.is_absolute() {
            expanded_path.to_path_buf()
        } else {
            cwd.join(expanded_path)
        }
    } else {
        let parent = expanded_path.parent().unwrap_or_else(|| Path::new(""));
        if parent.is_absolute() {
            parent.to_path_buf()
        } else {
            cwd.join(parent)
        }
    };

    let display_dir = if ends_with_sep {
        raw.to_string()
    } else if dir_raw.is_empty() {
        String::new()
    } else {
        format!("{}/", dir_raw.trim_end_matches('/'))
    };
    (search_dir, name_prefix.to_string(), display_dir)
}

fn join_display_path(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}{name}")
    }
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    } else if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .map(|p| p.join(rest).to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    } else {
        path.to_string()
    }
}

fn supported_image_mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn is_unsupported_binary(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("pdf" | "doc" | "docx" | "ppt" | "pptx" | "xls" | "xlsx" | "zip" | "gz" | "tar")
    )
}

fn text_block(text: String) -> ContentBlock {
    ContentBlock::Text {
        text,
        signature: None,
    }
}
