use std::ops::Range;

use crate::types::{Catalog, Segment, SegmentKind};
use crate::utils::extract_tokens;

pub struct Diagnostic<'a> {
    pub error: bool,
    pub message: String,
    pub line: u32,
    pub context: &'a str,
}

pub struct ApplyResult<'a> {
    pub output: String,
    pub applied: usize,
    pub fuzzy_skipped: usize,
    pub empty_skipped: usize,
    pub missing: usize,
    pub errors: usize,
    pub diagnostics: Vec<Diagnostic<'a>>,
}

pub(crate) fn plan_apply<'a>(
    source: &str,
    segments: &'a [Segment],
    catalog: &Catalog,
    check_only: bool,
) -> ApplyResult<'a> {
    let mut result = ApplyResult {
        output: String::new(),
        applied: 0,
        fuzzy_skipped: 0,
        empty_skipped: 0,
        missing: 0,
        errors: 0,
        diagnostics: Vec::new(),
    };
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();

    for seg in segments {
        let key = (seg.context.clone(), seg.source.clone());

        let entry = match catalog.get(&key) {
            Some(e) => e,
            None => {
                result.missing += 1;
                result.diagnostics.push(Diagnostic {
                    error: false,
                    message: format!("no translation for msgid {}", describe_msgid(&seg.source)),
                    line: seg.line,
                    context: &seg.context,
                });
                continue;
            }
        };

        if entry.flags.iter().any(|f| f.eq_ignore_ascii_case("fuzzy")) {
            result.fuzzy_skipped += 1;
            result.diagnostics.push(Diagnostic {
                error: false,
                message: "fuzzy entry skipped".to_string(),
                line: seg.line,
                context: &seg.context,
            });
            continue;
        }

        if entry.msgstr.trim().is_empty() {
            result.empty_skipped += 1;
            continue;
        }

        let mut ok = true;

        if let Err(e) = validate_tokens(seg, &entry.msgstr) {
            result.errors += 1;
            result.diagnostics.push(Diagnostic {
                error: true,
                message: e,
                line: seg.line,
                context: &seg.context,
            });
            ok = false;
        }
        if let SegmentKind::TranslatableLinkLabel = seg.kind
            && let Err(e) = validate_link_label(&entry.msgstr)
        {
            result.errors += 1;
            result.diagnostics.push(Diagnostic {
                error: true,
                message: e,
                line: seg.line,
                context: &seg.context,
            });
            ok = false;
        }

        if ok {
            replacements.push((seg.span.clone(), entry.msgstr.clone()));
            result.applied += 1;
        }
    }

    if !check_only {
        // 从后向前替换，避免前部替换改变后续 offset
        replacements.sort_by_key(|a| std::cmp::Reverse(a.0.start));
        let mut out = source.to_string();
        let mut last_start = usize::MAX;
        for (span, translation) in replacements {
            assert!(span.end <= last_start, "spans should not cross");

            out.replace_range(span.clone(), &translation);
            last_start = span.start;
        }

        result.output = out;
    }

    result
}

/// Validate `$identifier` / `_identifier` not broken
fn validate_tokens(seg: &Segment, msgstr: &str) -> Result<(), String> {
    let translated = extract_tokens(msgstr);
    let source = &seg.protected_tokens;

    let missing: Vec<&String> = source.iter().filter(|t| !translated.contains(t)).collect();
    let extra: Vec<&String> = translated.iter().filter(|t| !source.contains(t)).collect();

    if !missing.is_empty() {
        return Err(format!(
            "missing protected token(s) in translation: {}",
            missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !extra.is_empty() {
        return Err(format!(
            "unexpected protected token(s) in translation: {}",
            extra
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(())
}

/// Validate link label not contains `]]`、`->`、`<-`、`|` or \n
fn validate_link_label(msgstr: &str) -> Result<(), String> {
    if msgstr.contains("]]") {
        return Err("link label translation must not contain ']]'".into());
    }
    if msgstr.contains("->") || msgstr.contains("<-") {
        return Err("link label translation must not contain '->' or '<-'".into());
    }
    if msgstr.contains('|') {
        return Err("link label translation must not contain '|'".into());
    }
    if msgstr.contains('\n') || msgstr.contains('\r') {
        return Err("link label translation must not contain line breaks".into());
    }
    Ok(())
}

fn describe_msgid(s: &str) -> String {
    let mut escaped = String::new();
    for c in s.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(c),
        }
    }
    if escaped.chars().count() > 80 {
        let head: String = escaped.chars().take(77).collect();
        format!("\"{}...\"", head)
    } else {
        format!("\"{}\"", escaped)
    }
}
