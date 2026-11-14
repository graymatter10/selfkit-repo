//! Time the cold and warm data load on a data folder (performance check).
//! `SELFKIT_DATA_DIR=… cargo run -p reflect --example reflect_bench`
use reflect_lib::sources;
use std::time::Instant;

fn main() {
    let t = Instant::now();
    let s = sources::open_other("chrono").expect("chrono.db");
    let mut c = sources::ChronoCache::default();
    c.update(&s, 0);
    println!("chrono cold: {:?} ({} days)", t.elapsed(), c.days.len());
    let t = Instant::now();
    c.update(&s, 0);
    println!("chrono warm: {:?}", t.elapsed());
    let t = Instant::now();
    let h = sources::open_other("hisab").map(|s| sources::read_hisab(&s, "USD", 0));
    let a = sources::open_other("agenda").map(|s| sources::read_agenda(&s, 0));
    let d = sources::open_other("dusk").map(|s| sources::read_dusk(&s));
    println!("hisab+agenda+dusk: {:?} ({:?} spends, {:?} done, {:?} nights)", t.elapsed(), h.map(|h| h.spends.len()), a.map(|a| a.done.len()), d.map(|d| d.len()));
}
