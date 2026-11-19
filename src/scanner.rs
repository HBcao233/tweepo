use std::ops::Range;

use crate::types::{Segment, SegmentKind};
use crate::utils::extract_tokens;

/// Protected block macros
const PROTECTED_BLOCK_MACROS: [&str; 5] = ["script", "style", "widget", "link", "button"];

pub struct Scanner<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line_starts: Vec<usize>,
    passage: Option<String>,
    body_ordinal: u32,
    link_ordinal: u32,
    segments: Vec<Segment>,
}

impl<'a> Scanner<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, &b) in src.as_bytes().iter().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line_starts,
            passage: None,
            body_ordinal: 0,
            link_ordinal: 0,
            segments: Vec::new(),
        }
    }

    pub fn run(mut self) -> Vec<Segment> {
        while !self.at_end() {
            if self.at_line_start() && self.bytes[self.pos..].starts_with(b"::") {
                self.scan_header();
            } else if self.bytes[self.pos..].starts_with(b"[[") {
                self.scan_link();
            } else if self.inline_macro_end(self.pos).is_some() {
                self.scan_plain_text();
            } else if self.bytes[self.pos..].starts_with(b"<<") {
                self.scan_macro();
            } else if self.bytes[self.pos..].starts_with(b"<!--") {
                self.scan_block_comment(4, "-->");
            } else if self.bytes[self.pos..].starts_with(b"/*") {
                self.scan_block_comment(2, "*/");
            } else if self.bytes[self.pos..].starts_with(b"/%") {
                self.scan_block_comment(2, "%/");
            } else if self.bytes[self.pos..].starts_with(b"//") {
                self.scan_inline_comment();
            } else if self.bytes[self.pos] == b'<' {
                self.scan_html_tag();
            } else {
                self.scan_plain_text();
            }
        }
        self.segments
    }

    fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn is_line_start_at(&self, pos: usize) -> bool {
        pos == 0 || self.bytes[pos - 1] == b'\n'
    }

    fn at_line_start(&self) -> bool {
        self.is_line_start_at(self.pos)
    }

    fn move_to(&mut self, pos: usize) {
        debug_assert!(pos >= self.pos);
        self.pos = pos.min(self.bytes.len());
    }

    fn line_at(&self, pos: usize) -> u32 {
        self.line_starts.partition_point(|&s| s <= pos) as u32
    }

    fn find_line_end(&self, from: usize) -> usize {
        self.bytes[from..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| from + i)
            .unwrap_or(self.bytes.len())
    }

    /// Find `close` from `from`
    ///
    /// Used in [`Self::scan_link`].
    fn find_close(&self, from: usize, close: &[u8]) -> Option<usize> {
        let mut i = from;
        while i < self.bytes.len() {
            if self.bytes[i..].starts_with(close) {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    /// Find `close` from `from`, skip content in `"..."` or `'...'`
    ///
    /// Used in [`Self::scan_macro`].
    fn find_close_macro(&self, from: usize, close: &[u8]) -> Option<usize> {
        let mut i = from;
        let mut dq = false;
        let mut sq = false;
        while i < self.bytes.len() {
            let b = self.bytes[i];
            if dq {
                if b == b'"' {
                    dq = false;
                }
                i += 1;
                continue;
            }
            if sq {
                if b == b'\'' {
                    sq = false;
                }
                i += 1;
                continue;
            }
            if b == b'"' {
                dq = true;
                i += 1;
                continue;
            }
            if b == b'\'' {
                sq = true;
                i += 1;
                continue;
            }
            if self.bytes[i..].starts_with(close) {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    /// 若 `pos` 处是行内输出宏（`<<=`、`<<-`、`<<print `），返回宏结束位置（`>>` 之后）
    fn inline_macro_end(&self, pos: usize) -> Option<usize> {
        let rest = &self.bytes[pos..];
        let body_start = if rest.starts_with(b"<<=") || rest.starts_with(b"<<-") {
            pos + 3
        } else if self.starts_with_ignore_case(pos, "<<print") {
            pos + 7
        } else {
            return None;
        };
        self.find_close_macro(body_start, b">>")
    }

    /// Passage header: `:: Name`
    fn scan_header(&mut self) {
        let start = self.pos;
        let line_end = self.find_line_end(start);
        let after = &self.src[start + 2..line_end];
        let name_end = after.find('[').unwrap_or(after.len());
        let name = after[..name_end].trim().to_string();

        self.passage = Some(name);
        self.body_ordinal = 0;
        self.link_ordinal = 0;

        self.move_to(line_end);
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'\n' {
            self.move_to(self.pos + 1);
        }
    }

    /// links: `[[...]]`
    fn scan_link(&mut self) {
        let start = self.pos;
        let close = match self.find_close(start + 2, b"]]") {
            Some(p) => p,
            None => {
                self.move_to(start + 2);
                return;
            }
        };

        let inner_start = start + 2;
        let inner = &self.src[inner_start..close];

        if let Some((label_rel, target, _setter)) = parse_link(inner) {
            let label_start = inner_start + label_rel.start;
            let label_end = inner_start + label_rel.end;
            let (ts, te) = trim_bounds(self.src, label_start, label_end);

            if ts < te {
                let source = self.src[ts..te].to_string();
                let tokens = extract_tokens(&source);
                let passage = self.passage.clone().unwrap_or_default();
                let ordinal = self.link_ordinal;
                self.link_ordinal += 1;
                let context = format!("passage:{}/link:{}:{}", passage, target, ordinal);
                let line = self.line_at(ts);

                self.segments.push(Segment {
                    kind: SegmentKind::TranslatableLinkLabel,
                    span: ts..te,
                    source,
                    context,
                    line,
                    protected_tokens: tokens,
                    passage,
                    link_target: Some(target),
                });
            }
        }

        self.move_to(close + 2);
    }

    /// Find first macro arg in `[header_start, header_end)`
    fn find_macro_first_arg(
        &self,
        header_start: usize,
        header_end: usize,
    ) -> Option<(usize, usize)> {
        let mut i = header_start;

        // 跳过宏名
        while i < header_end && (self.bytes[i].is_ascii_alphanumeric() || self.bytes[i] == b'_') {
            i += 1;
        }

        if self.bytes[i] == b' ' {
            i += 1;
        }

        let q_start = i;
        let mut quote = None;
        while i < header_end {
            let b = self.bytes[i];
            if quote.is_none() && b == b' ' {
                break;
            }
            if [b'"', b'\'', b'`'].contains(&b) {
                if quote.is_some() {
                    i += 1;
                    break;
                }
                quote = Some(b);
            }
            i += 1;
        }

        if q_start != i {
            Some((q_start, i))
        } else {
            None
        }
    }

    /// Macros: `<<...>>`
    fn scan_macro(&mut self) {
        let start = self.pos;

        // 找起始标签的结束 `>>`
        let close = match self.find_close_macro(start + 2, b">>") {
            Some(p) => p,
            None => {
                // 未闭合，'<' 按普通字符处理
                self.move_to(start + 1);
                return;
            }
        };

        let header = self.src[start + 2..close].trim_start();

        // 闭合标签, 直接跳过
        if header.starts_with('/') {
            self.move_to(close + 2);
            return;
        }

        let name = parse_macro_name(header);

        // 透明宏（if / for / set / print / ...）：只跳过起始标签，
        // 内部内容继续由主循环按普通文本处理。
        if !PROTECTED_BLOCK_MACROS
            .iter()
            .any(|m| m.eq_ignore_ascii_case(&name))
        {
            self.move_to(close + 2);
            return;
        }

        // 块级宏：link / button 提取第一个参数作为可翻译 label
        if (name.eq_ignore_ascii_case("link") || name.eq_ignore_ascii_case("button"))
            && let Some((label_start, label_end)) = self.find_macro_first_arg(start + 2, close)
        {
            let (ts, te) = trim_bounds(self.src, label_start, label_end);
            if ts < te {
                let source = self.src[ts..te].to_string();
                let tokens = extract_tokens(&source);
                let passage = self.passage.clone().unwrap_or_default();
                let ordinal = self.link_ordinal;
                self.link_ordinal += 1;
                let context = format!("passage:{}/linklabel:{}", passage, ordinal);
                let line = self.line_at(ts);

                self.segments.push(Segment {
                    kind: SegmentKind::TranslatableLinkLabel,
                    span: ts..te,
                    source,
                    context,
                    line,
                    protected_tokens: tokens,
                    passage,
                    link_target: None,
                });
            }
        }

        // 查找匹配的 `<</name>>`，整体跳过块内内容
        let close_tag = format!("<</{}", name.to_ascii_lowercase());
        if let Some(end) = self.find_case_insensitive_from(close + 2, &close_tag)
            && let Some(gt) = self.find_close(end + 2, b">>")
        {
            self.move_to(gt + 2);
            return;
        }

        // 没找到闭合标签：退化为只跳过起始宏，避免死循环
        self.move_to(close + 2);
    }

    /// 从 `from` 起做 ASCII 大小写不敏感查找。
    fn find_case_insensitive_from(&self, from: usize, needle: &str) -> Option<usize> {
        if from >= self.src.len() {
            return None;
        }
        let lower_hay = self.src[from..].to_ascii_lowercase();
        let lower_needle = needle.to_ascii_lowercase();
        lower_hay.find(&lower_needle).map(|p| from + p)
    }

    /// Block comment: `<!--...-->` / `/*...*/` `/%...%/`
    fn scan_block_comment(&mut self, open_len: usize, close: &str) {
        let start = self.pos;
        match self.src[start + open_len..].find(close) {
            Some(p) => self.move_to(start + open_len + p + close.len()),
            None => self.move_to(self.bytes.len()),
        }
    }

    /// Inline comment: `//...`
    fn scan_inline_comment(&mut self) {
        let start = self.pos;
        let line_end = self.find_line_end(start);
        self.move_to(line_end);
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'\n' {
            self.move_to(self.pos + 1);
        }
    }

    /// HTML tag
    fn scan_html_tag(&mut self) {
        let start = self.pos;

        if self.starts_with_ignore_case(start, "<script")
            && let Some(end) = self.find_block_end(start + 7, "</script")
        {
            self.move_to(end);
            return;
        }
        if self.starts_with_ignore_case(start, "<style")
            && let Some(end) = self.find_block_end(start + 6, "</style")
        {
            self.move_to(end);
            return;
        }

        // 普通标签：扫描到非引号状态下的 '>'
        let mut i = start + 1;
        let mut dq = false;
        let mut sq = false;
        while i < self.bytes.len() {
            let b = self.bytes[i];
            if dq {
                if b == b'"' {
                    dq = false;
                }
            } else if sq {
                if b == b'\'' {
                    sq = false;
                }
            } else if b == b'"' {
                dq = true;
            } else if b == b'\'' {
                sq = true;
            } else if b == b'>' {
                self.move_to(i + 1);
                return;
            }
            i += 1;
        }
        // 未闭合：将 '<' 视为普通字符
        self.move_to(start + 1);
    }

    fn starts_with_ignore_case(&self, pos: usize, s: &str) -> bool {
        let end = pos + s.len();
        end <= self.bytes.len()
            && self
                .bytes
                .get(pos..end)
                .and_then(|rest| rest.get(..s.len()))
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(s.as_bytes()))
    }

    fn find_block_end(&self, from: usize, close_tag: &str) -> Option<usize> {
        let lower = self.src[from..].to_ascii_lowercase();
        let rel = lower.find(close_tag)?;
        let close_start = from + rel;
        self.src[close_start..]
            .find('>')
            .map(|p| close_start + p + 1)
    }

    /// Plain text
    fn scan_plain_text(&mut self) {
        let start = self.pos;
        let mut i = start;

        while i < self.bytes.len() {
            // 行首 "::" 视为新的 passage header
            if self.is_line_start_at(i) && self.bytes[i..].starts_with(b"::") {
                break;
            }

            let b = self.bytes[i];
            // "[[" 中断普通文本
            if b == b'[' && self.bytes[i..].starts_with(b"[[") {
                break;
            }
            // "<" 交给 HTML/宏分支
            if b == b'<' {
                // 行内输出宏 `<<=...>>` 视为正文中的占位符，不中断
                if let Some(end) = self.inline_macro_end(i) {
                    i = end;
                    continue;
                }
                break;
            }
            // "//" 中断普通文本（排除 URL 中的 ://）
            if b == b'/'
                && self.bytes[i..].starts_with(b"//")
                && (i == 0 || self.bytes[i - 1] != b':')
            {
                break;
            }
            // "/*", "/%" 中断普通文本
            if b == b'/'
                && (self.bytes[i..].starts_with(b"/*") || self.bytes[i..].starts_with(b"/%"))
            {
                break;
            }
            i += 1;
        }

        if i == start {
            // 保险：避免死循环
            self.move_to(start + 1);
            return;
        }

        let (ts, te) = trim_bounds(self.src, start, i);
        if ts < te {
            let source = self.src[ts..te].to_string();
            let tokens = extract_tokens(&source);
            let passage = self.passage.clone().unwrap_or_default();
            let ordinal = self.body_ordinal;
            self.body_ordinal += 1;
            let context = format!("passage:{}/body:{}", passage, ordinal);
            let line = self.line_at(ts);

            self.segments.push(Segment {
                kind: SegmentKind::TranslatableBody,
                span: ts..te,
                source,
                context,
                line,
                protected_tokens: tokens,
                passage,
                link_target: None,
            });
        }

        self.move_to(i);
    }
}

/// Parse content in `[[...|...]]` or `[[...|...][]]`.
fn parse_link(inner: &str) -> Option<(Range<usize>, String, Option<String>)> {
    if let Some(i) = inner.find("->") {
        let (target, setter) = split_setter(&inner[i + 2..]);
        Some((0..i, target, setter))
    } else if let Some(i) = inner.find("<-") {
        let (target, setter) = split_setter(&inner[..i]);
        Some((i + 2..inner.len(), target, setter))
    } else if let Some(i) = inner.find('|') {
        let (target, setter) = split_setter(&inner[i + 1..]);
        Some((0..i, target, setter))
    } else {
        None
    }
}

fn split_setter(t: &str) -> (String, Option<String>) {
    if let Some((target, setter)) = t.split_once("][") {
        (target.to_string(), Some(setter.to_string()))
    } else {
        (t.to_string(), None)
    }
}

/// trim in `[start, end)`
fn trim_bounds(src: &str, start: usize, end: usize) -> (usize, usize) {
    let s = &src[start..end];
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return (start, start);
    }
    let lead = s.len() - s.trim_start().len();
    let trail = s.len() - s.trim_end().len();
    (start + lead, end - trail)
}

/// Parse macro name in `<<...>>`
fn parse_macro_name(header: &str) -> String {
    let b = header.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    header[..i].to_string()
}
