//! Fuzz harness: parse arbitrary bytes as a tar archive and walk every entry.
//! not just another harness
//! The `tar` crate is real Rust code (good for a “Rust as target” example).LibAFL has libpng as target,
//! so i thought of using tar as target. Work is bounded and
//! slightly amplified so runs are not trivially cheap: cap input size, read full entry bodies, and
//! fold bytes so the CPU does meaningful work per entry. learned this from the libafl docs.

use std::io::{Cursor, Read};

use tar::Archive;

/// Upper bound on bytes fed to the archive parser (avoids huge allocations from fuzz input).
const MAX_INPUT: usize = 256 * 1024;

/// Run tar parsing + per-entry scanning. `on_signal` is used for a tiny synthetic coverage map.
pub fn run_from_input(data: &[u8], on_signal: impl Fn(usize)) {
    on_signal(0);

    let data = if data.len() > MAX_INPUT {
        on_signal(1);
        &data[..MAX_INPUT]
    } else {
        data
    };

    let mut archive = Archive::new(Cursor::new(data));
    on_signal(2);

    let Ok(entries) = archive.entries() else {
        on_signal(3);
        return;
    };

    for mut entry in entries.filter_map(Result::ok) {
        on_signal(4);
        let _ = entry.path();
        on_signal(5);

        let mut body = Vec::new();
        if entry.read_to_end(&mut body).is_err() {
            on_signal(6);
            continue;
        }

        on_signal(7);
        // Cheap but non-trivial per-byte work (keeps the target “slow” vs a pure parse-only pass).
        let mut acc = 0xcbf29ce484222325u64;
        for chunk in body.chunks(2048) {
            for &b in chunk {
                acc ^= b as u64;
                acc = acc.wrapping_mul(0x100000001b3);
            }
        }
        let _ = acc;
        on_signal(8);
    }

    on_signal(9);
}
