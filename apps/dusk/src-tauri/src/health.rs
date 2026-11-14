//! Extension point: sleep sessions from Android Health Connect (smartwatches).
//! PLANNING marks this as "later", so nothing reads Health Connect yet.
//!
//! How to add it (docs/apps/dusk/estimation.md § Health Connect):
//! 1. Kotlin: add `androidx.health.connect:connect-client`, request
//!    `android.permission.health.READ_SLEEP`, read `SleepSessionRecord`s in
//!    the `DuskUsage` bridge (MainActivity.kt) as JSON `[{start,end,tz}]`.
//! 2. Frontend: pass them to the `external_sleep` command next to the usage events.
//! 3. Return them from `sessions()` below (store them in a `health_sleep`
//!    collection so they sync to the desktop). `model::night_view` already ranks
//!    them above the device estimate and below the manual log.

use crate::model::{ExternalSleep, Ms};

/// External sleep sessions overlapping [from, to). Empty until Health Connect is wired.
pub fn sessions(_store: &selfkit_core::Store, _from: Ms, _to: Ms) -> Vec<ExternalSleep> {
    Vec::new()
}
