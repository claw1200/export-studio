//! The audio callback must not allocate. Counted with a global allocator, on the calling thread
//! only, while the callback runs.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.with(Cell::get) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn the_callback_does_not_allocate() {
    let rig = Rig::new();
    rig.load(&dc("alloc", RATE * 4));
    rig.engine.play(0);
    let mut buffer = vec![0.0_f32; 512 * 2];
    // The first callback builds its level smoother; everything after is steady state.
    rig.sink.pull_into(&mut buffer);
    for round in 0..200 {
        if round == 50 {
            rig.engine.seek(0, 2000.0);
        }
        if round == 120 {
            rig.engine.pause(0);
        }
        if round == 150 {
            rig.engine.play(0);
        }
        COUNTING.with(|c| c.set(true));
        rig.sink.pull_into(&mut buffer);
        COUNTING.with(|c| c.set(false));
        std::thread::sleep(std::time::Duration::from_micros(300));
    }
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
}
