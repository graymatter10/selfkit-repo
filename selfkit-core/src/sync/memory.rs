//! In-memory [`Remote`] for tests and offline demos.
use std::sync::Mutex;

use super::{Remote, RemoteRecord};
use crate::Result;

#[derive(Default)]
pub struct MemoryRemote {
    inner: Mutex<(Vec<RemoteRecord>, i64)>,
}

impl MemoryRemote {
    pub fn rows(&self) -> Vec<RemoteRecord> {
        self.inner.lock().unwrap().0.clone()
    }
}

impl Remote for MemoryRemote {
    async fn push(&self, rec: &RemoteRecord, base_rev: i64) -> Result<Option<i64>> {
        let mut g = self.inner.lock().unwrap();
        let (rows, seq) = &mut *g;
        let pos = rows.iter().position(|r| r.app == rec.app && r.id == rec.id);
        let ok = match pos {
            None => base_rev == 0,
            Some(i) => rows[i].rev == base_rev,
        };
        if !ok {
            return Ok(None);
        }
        *seq += 1;
        let mut r = rec.clone();
        r.rev = *seq;
        match pos {
            Some(i) => rows[i] = r,
            None => rows.push(r),
        }
        Ok(Some(*seq))
    }

    async fn pull(&self, app: &str, since: i64, limit: i64) -> Result<Vec<RemoteRecord>> {
        let g = self.inner.lock().unwrap();
        let mut v: Vec<_> = g.0.iter().filter(|r| r.app == app && r.rev > since).cloned().collect();
        v.sort_by_key(|r| r.rev);
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn fetch(&self, app: &str, id: &str) -> Result<Option<RemoteRecord>> {
        Ok(self.inner.lock().unwrap().0.iter().find(|r| r.app == app && r.id == id).cloned())
    }
}
