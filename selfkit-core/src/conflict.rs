//! Helpers for the conflict screen: field-by-field diff/merge and line diffs
//! for long text. Storage of conflicts lives in `store`.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Side {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FieldDiff {
    pub key: String,
    pub local: Option<Value>,
    pub remote: Option<Value>,
}

/// Top-level fields that differ between two JSON objects.
pub fn field_diffs(local: &Value, remote: &Value) -> Vec<FieldDiff> {
    let empty = Map::new();
    let (l, r) = (local.as_object().unwrap_or(&empty), remote.as_object().unwrap_or(&empty));
    let keys: BTreeSet<&String> = l.keys().chain(r.keys()).collect();
    keys.into_iter()
        .filter(|k| l.get(*k) != r.get(*k))
        .map(|k| FieldDiff { key: k.clone(), local: l.get(k).cloned(), remote: r.get(k).cloned() })
        .collect()
}

/// Build a merged object: start from `local`, take each field listed in
/// `choices` from the chosen side (a missing value on that side removes the field).
pub fn merge_fields(local: &Value, remote: &Value, choices: &HashMap<String, Side>) -> Value {
    let mut out = local.as_object().cloned().unwrap_or_default();
    let empty = Map::new();
    let r = remote.as_object().unwrap_or(&empty);
    for (k, side) in choices {
        if *side == Side::Remote {
            match r.get(k) {
                Some(v) => {
                    out.insert(k.clone(), v.clone());
                }
                None => {
                    out.remove(k);
                }
            }
        }
    }
    Value::Object(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LineTag {
    Same,
    Local,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    pub tag: LineTag,
    pub text: String,
}

/// Line-by-line diff of long text (e.g. task descriptions).
pub fn line_diff(local: &str, remote: &str) -> Vec<DiffLine> {
    // A last line without '\n' must still equal the same line elsewhere.
    let nl = |s: &str| if s.is_empty() || s.ends_with('\n') { s.to_string() } else { format!("{s}\n") };
    let (local, remote) = (nl(local), nl(remote));
    similar::TextDiff::from_lines(&local, &remote)
        .iter_all_changes()
        .map(|c| DiffLine {
            tag: match c.tag() {
                similar::ChangeTag::Equal => LineTag::Same,
                similar::ChangeTag::Delete => LineTag::Local,
                similar::ChangeTag::Insert => LineTag::Remote,
            },
            text: c.value().trim_end_matches('\n').to_string(),
        })
        .collect()
}

/// Rebuild text from a diff, keeping `Same` lines plus the lines whose index
/// is in `keep` (the user's ticks in the merge view).
pub fn apply_line_picks(diff: &[DiffLine], keep: &BTreeSet<usize>) -> String {
    diff.iter()
        .enumerate()
        .filter(|(i, l)| l.tag == LineTag::Same || keep.contains(i))
        .map(|(_, l)| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fields() {
        let l = json!({"title": "a", "done": false, "tag": "x"});
        let r = json!({"title": "b", "done": false, "prio": 1});
        let d = field_diffs(&l, &r);
        assert_eq!(d.iter().map(|f| f.key.as_str()).collect::<Vec<_>>(), ["prio", "tag", "title"]);
        let m = merge_fields(&l, &r, &HashMap::from([("title".into(), Side::Remote), ("prio".into(), Side::Remote)]));
        assert_eq!(m, json!({"title": "b", "done": false, "tag": "x", "prio": 1}));
    }

    #[test]
    fn lines() {
        let d = line_diff("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(d.len(), 4);
        let keep: BTreeSet<usize> = d.iter().enumerate().filter(|(_, l)| l.tag == LineTag::Remote).map(|(i, _)| i).collect();
        assert_eq!(apply_line_picks(&d, &keep), "a\nB\nc");
    }
}
