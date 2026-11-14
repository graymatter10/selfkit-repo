//! One LAN sync session over an encrypted [`Channel`]: exchange hellos, then
//! each side pulls the other's changes for every app both have (by the
//! sender's local change sequence), applying them with version histories.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use super::channel::Channel;
use crate::store::{Applied, Record};
use crate::{Error, Result, Store};

pub const PROTO: u32 = 1;
const PAGE: u32 = 500;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum Msg {
    Hello { device_id: String, name: String, apps: Vec<String>, proto: u32 },
    Pull { app: String, since: i64 },
    Batch { app: String, records: Vec<Record>, upto: i64, more: bool },
    YourTurn,
    Bye,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct SessionReport {
    pub peer_id: String,
    pub peer_name: String,
    pub received: u64,
    pub sent: u64,
    pub conflicts: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Client,
    Server,
}

fn cursor_key(peer: &str) -> String {
    format!("lan.cursor.{peer}")
}

async fn pull_phase<S: AsyncRead + AsyncWrite + Unpin + Send>(
    ch: &mut Channel<S>,
    peer: &str,
    apps: &[String],
    stores: &mut HashMap<String, Store>,
    rep: &mut SessionReport,
) -> Result<()> {
    for app in apps {
        let store = stores.get_mut(app).expect("common app");
        loop {
            let since: i64 = store.kv_get(&cursor_key(peer))?.and_then(|s| s.parse().ok()).unwrap_or(0);
            ch.send(&Msg::Pull { app: app.clone(), since }).await?;
            let Msg::Batch { app: got, records, upto, more } = ch.recv().await? else {
                return Err(Error::Format("lan: expected batch".into()));
            };
            if &got != app {
                return Err(Error::Format("lan: batch for wrong app".into()));
            }
            let tx = store.conn().unchecked_transaction()?;
            for r in &records {
                match store.apply_peer(r)? {
                    Applied::Inserted | Applied::Updated => rep.received += 1,
                    Applied::Conflict => rep.conflicts += 1,
                    Applied::Unchanged => {}
                }
            }
            store.kv_set(&cursor_key(peer), &upto.to_string())?;
            tx.commit()?;
            if !more {
                break;
            }
        }
    }
    Ok(())
}

/// Answer Pulls until the peer says YourTurn/Bye. Returns the ending message.
async fn serve_phase<S: AsyncRead + AsyncWrite + Unpin + Send>(
    ch: &mut Channel<S>,
    apps: &[String],
    stores: &mut HashMap<String, Store>,
    rep: &mut SessionReport,
) -> Result<Msg> {
    loop {
        match ch.recv().await? {
            Msg::Pull { app, since } => {
                if !apps.contains(&app) {
                    return Err(Error::Format("lan: pull for unshared app".into()));
                }
                let changes = stores[&app].changes_since(since, PAGE)?;
                let more = changes.len() as u32 == PAGE;
                let upto = changes.last().map(|(_, l)| *l).unwrap_or(since);
                rep.sent += changes.len() as u64;
                let records = changes.into_iter().map(|(r, _)| r).collect();
                ch.send(&Msg::Batch { app, records, upto, more }).await?;
            }
            m @ (Msg::YourTurn | Msg::Bye) => return Ok(m),
            _ => return Err(Error::Format("lan: unexpected message".into())),
        }
    }
}

/// Run a full two-way sync. `stores` holds this device's open app stores.
pub async fn run<S: AsyncRead + AsyncWrite + Unpin + Send>(
    mut ch: Channel<S>,
    role: Role,
    my_id: &str,
    my_name: &str,
    stores: &mut HashMap<String, Store>,
) -> Result<SessionReport> {
    let mut my_apps: Vec<String> = stores.keys().cloned().collect();
    my_apps.sort();
    ch.send(&Msg::Hello { device_id: my_id.into(), name: my_name.into(), apps: my_apps.clone(), proto: PROTO }).await?;
    let Msg::Hello { device_id, name, apps, proto } = ch.recv().await? else {
        return Err(Error::Format("lan: expected hello".into()));
    };
    if proto != PROTO {
        return Err(Error::Format(format!("lan: peer speaks protocol {proto}, we speak {PROTO}")));
    }
    if device_id == my_id {
        return Err(Error::Format("lan: connected to ourselves".into()));
    }
    let common: Vec<String> = my_apps.into_iter().filter(|a| apps.contains(a)).collect();
    let mut rep = SessionReport { peer_id: device_id.clone(), peer_name: name, ..Default::default() };
    match role {
        Role::Client => {
            pull_phase(&mut ch, &device_id, &common, stores, &mut rep).await?;
            ch.send(&Msg::YourTurn).await?;
            serve_phase(&mut ch, &common, stores, &mut rep).await?;
        }
        Role::Server => {
            if let Msg::YourTurn = serve_phase(&mut ch, &common, stores, &mut rep).await? {
                pull_phase(&mut ch, &device_id, &common, stores, &mut rep).await?;
                ch.send(&Msg::Bye).await?;
            }
        }
    }
    Ok(rep)
}
