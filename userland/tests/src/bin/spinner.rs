//! A bare spin that counts on the test page and never traps. `arg` is an id the program stores in
//! the page and re-reads on every iteration, so a thread running under another space's page
//! tables sees a foreign id and exits 1. The redirect handler exits with the interrupted pc, so
//! a pc inside the image text is the pass condition.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::sync::atomic::Ordering::Relaxed;

use libsys::test_page;

fn main(id: u64) -> u64 {
    let page = test_page();
    let id = id as u32;
    page[1].store(id, Relaxed);
    loop {
        page[0].fetch_add(1, Relaxed);
        if page[1].load(Relaxed) != id {
            return 1;
        }
    }
}

libsys::entry!(main);
