//! Data for the conflict screen (Keep A / Keep B / Merge field by field, line diff
//! for long text). A = this device's version, B = the other device's.

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use selfkit_core::conflict::{self, DiffLine, Side};
use selfkit_core::store::Resolution;
use selfkit_core::{Error, Result, Store};

/// Strings with a newline or longer than this get a line diff.
const LONG_TEXT: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Version {
    pub device_id: String,
    pub updated_at: i64,
    pub tz_offset: i32,
    pub deleted: bool,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FieldView {
    pub key: String,
    pub a: Option<Value>,
    pub b: Option<Value>,
    /// Present for long text on both sides: tick lines to keep.
    pub lines: Option<Vec<DiffLine>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConflictView {
    /// Which DB: the app's own name or "settings".
    pub store: String,
    pub id: String,
    pub collection: String,
    /// "server" or "peer".
    pub source: String,
    pub a: Version,
    pub b: Version,
    /// Only fields that differ.
    pub fields: Vec<FieldView>,
    /// Field names that are equal on both sides.
    pub same: Vec<String>,
    /// One side deleted the item (edit vs delete).
    pub delete_vs_edit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "choice", rename_all = "snake_case")]
pub enum Choice {
    KeepA,
    KeepB,
    /// `fields`: field -> "a" | "b" (unlisted fields keep A).
    /// `lines`: field -> indices into that field's `lines` to keep (Same lines always stay).
    Merge {
        #[serde(default)]
        fields: HashMap<String, String>,
        #[serde(default)]
        lines: HashMap<String, BTreeSet<usize>>,
    },
}

/// Bookkeeping fields the user never picks between (bugs.md QA-U2): they follow the
/// chosen version (Merge keeps A's, which the store restamps anyway).
fn hidden(key: &str) -> bool {
    matches!(key, "id" | "created_at" | "updated_at" | "tz_offset")
}

fn is_long(v: &Option<Value>) -> bool {
    matches!(v, Some(Value::String(s)) if s.contains('\n') || s.chars().count() > LONG_TEXT)
}

pub fn view(store_name: &str, store: &Store) -> Result<Vec<ConflictView>> {
    let mut out = vec![];
    for c in store.conflicts()? {
        let diffs = conflict::field_diffs(&c.local.data, &c.remote.data);
        let fields = diffs
            .into_iter()
            .filter(|d| !hidden(&d.key))
            .map(|d| {
                let lines = (is_long(&d.local) && is_long(&d.remote)).then(|| {
                    conflict::line_diff(d.local.as_ref().and_then(Value::as_str).unwrap_or(""), d.remote.as_ref().and_then(Value::as_str).unwrap_or(""))
                });
                FieldView { key: d.key, a: d.local, b: d.remote, lines }
            })
            .collect::<Vec<_>>();
        let same = c
            .local
            .data
            .as_object()
            .map(|o| o.keys().filter(|k| !hidden(k) && !fields.iter().any(|f| &f.key == *k)).cloned().collect())
            .unwrap_or_default();
        let v = |r: &selfkit_core::Record| Version {
            device_id: r.device_id.clone(),
            updated_at: r.updated_at,
            tz_offset: r.tz_offset,
            deleted: r.deleted,
            data: r.data.clone(),
        };
        out.push(ConflictView {
            store: store_name.into(),
            id: c.local.id.clone(),
            collection: c.local.collection.clone(),
            source: c.source.clone(),
            delete_vs_edit: c.local.deleted != c.remote.deleted,
            a: v(&c.local),
            b: v(&c.remote),
            fields,
            same,
        });
    }
    Ok(out)
}

pub fn resolve(store: &Store, id: &str, choice: &Choice) -> Result<()> {
    let how = match choice {
        // Keeping our edit over the other side's delete brings the item back everywhere.
        Choice::KeepA => Resolution::KeepLocal,
        Choice::KeepB => Resolution::KeepRemote,
        Choice::Merge { fields, lines } => {
            let c = store.conflicts()?.into_iter().find(|c| c.local.id == id).ok_or_else(|| Error::NotFound(id.into()))?;
            let picks: HashMap<String, Side> = fields
                .iter()
                .map(|(k, s)| (k.clone(), if s.eq_ignore_ascii_case("b") { Side::Remote } else { Side::Local }))
                .collect();
            let mut merged = conflict::merge_fields(&c.local.data, &c.remote.data, &picks);
            for (k, keep) in lines {
                let (a, b) = (c.local.data.get(k).and_then(Value::as_str), c.remote.data.get(k).and_then(Value::as_str));
                if let (Some(a), Some(b)) = (a, b) {
                    let text = conflict::apply_line_picks(&conflict::line_diff(a, b), keep);
                    if let Some(o) = merged.as_object_mut() {
                        o.insert(k.clone(), Value::String(text));
                    }
                }
            }
            Resolution::Merged(merged)
        }
    };
    store.resolve_conflict(id, how)

}
