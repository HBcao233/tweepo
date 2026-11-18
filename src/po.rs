use std::collections::HashMap;

use crate::types::{Catalog, CatalogEntry, Field};

/// Parse .po file
pub fn parse_po(text: &str) -> Catalog {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut catalog: Catalog = HashMap::new();
    let mut current: Option<CatalogEntry> = None;
    let mut field = Field::None;
    let mut msgid_started = false;
    let mut pending_flags: Vec<String> = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim_start();

        // empty line: entry end
        if trimmed.is_empty() {
            flush_entry(&mut current, &mut catalog);
            field = Field::None;
            msgid_started = false;
            pending_flags.clear();
            continue;
        }

        // comments
        if let Some(rest) = trimmed.strip_prefix('#') {
            if let Some(flags_str) = rest.strip_prefix(',') {
                for flag in flags_str.split(',') {
                    let f = flag.trim();
                    if !f.is_empty() {
                        pending_flags.push(f.to_string());
                    }
                }
            }
            continue;
        }

        // msgctxt
        if let Some(rest) = trimmed.strip_prefix("msgctxt ") {
            flush_entry(&mut current, &mut catalog);
            current = Some(CatalogEntry {
                context: Some(parse_po_string(rest)),
                msgid: String::new(),
                msgstr: String::new(),
                flags: std::mem::take(&mut pending_flags),
            });
            field = Field::Context;
            msgid_started = false;
            continue;
        }

        // msgid
        if let Some(rest) = trimmed.strip_prefix("msgid ") {
            if current.is_none() || msgid_started {
                flush_entry(&mut current, &mut catalog);
                current = Some(CatalogEntry {
                    context: None,
                    msgid: String::new(),
                    msgstr: String::new(),
                    flags: std::mem::take(&mut pending_flags),
                });
            }
            if let Some(entry) = current.as_mut() {
                entry.msgid = parse_po_string(rest);
            }
            field = Field::Msgid;
            msgid_started = true;
            continue;
        }

        // msgstr
        if let Some(rest) = trimmed.strip_prefix("msgstr ") {
            if let Some(entry) = current.as_mut() {
                entry.msgstr = parse_po_string(rest);
            }
            field = Field::Msgstr;
            continue;
        }

        // continuation string
        if trimmed.starts_with('"') {
            let value = parse_po_string(trimmed);
            if let Some(entry) = current.as_mut() {
                match field {
                    Field::Context => entry
                        .context
                        .get_or_insert_with(String::new)
                        .push_str(&value),
                    Field::Msgid => entry.msgid.push_str(&value),
                    Field::Msgstr => entry.msgstr.push_str(&value),
                    Field::None => {}
                }
            }
            continue;
        }

        // TODO: msgid_plural、msgstr[0]...
    }

    flush_entry(&mut current, &mut catalog);
    catalog
}

fn flush_entry(current: &mut Option<CatalogEntry>, catalog: &mut Catalog) {
    if let Some(entry) = current.take()
        && !entry.msgid.is_empty()
    {
        let key = (
            entry.context.clone().unwrap_or_default(),
            entry.msgid.clone(),
        );
        catalog.insert(key, entry);
    }
}

/// Parse PO string literal `"..."`
fn parse_po_string(s: &str) -> String {
    let s = s.trim();
    if s.len() < 2 || !s.starts_with('"') || !s.ends_with('"') {
        return String::new();
    }
    po_unescape(&s[1..s.len() - 1])
}

fn po_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('a') => out.push('\u{0007}'),
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000C}'),
            Some('v') => out.push('\u{000B}'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
