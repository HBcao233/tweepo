use std::collections::HashMap;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    TranslatableBody,
    TranslatableLinkLabel,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Segment {
    pub kind: SegmentKind,
    pub span: Range<usize>,
    pub source: String,
    pub context: String,
    pub line: u32,
    pub protected_tokens: Vec<String>,
    pub passage: String,
    pub link_target: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub context: Option<String>,
    pub msgid: String,
    pub msgstr: String,
    pub flags: Vec<String>,
    pub comments: Vec<String>,
}

pub type Catalog = HashMap<(String, String), CatalogEntry>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    None,
    Context,
    Msgid,
    Msgstr,
}
