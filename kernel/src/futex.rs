//! Futex: wait, wake and requeue keyed by physical address, so two address spaces that map one
//! frame meet on the same word. Copy-on-write is broken before the key is taken: otherwise a
//! waiter keys the shared frame, the waker's store copies the page, and the wake finds nobody.

use alloc::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ostd::{
    mm::{
        HasPaddr, PAGE_SIZE, UFrame, Vaddr, VmIo, VmSpace, is_in_user_space,
        vm_space::VmQueriedItem,
    },
    sync::{SpinLock, WaitQueue},
    task::disable_preempt,
};

use crate::{
    thread::{Interrupted, Thread},
    vm::handle_cow_fault,
};

#[derive(Debug, PartialEq, Eq)]
pub enum FutexError {
    /// The word did not hold the expected value.
    Again,
    Fault,
    /// Misaligned address.
    Inval,
    Interrupted,
    /// Breaking copy-on-write exceeded the agent's memory budget.
    Budget,
}

impl FutexError {
    pub fn errno(&self) -> isize {
        match self {
            Self::Again => 11,
            Self::Fault => 14,
            Self::Inval => 22,
            Self::Interrupted => 4,
            Self::Budget => 12,
        }
    }
}

impl From<Interrupted> for FutexError {
    fn from(_: Interrupted) -> Self {
        Self::Interrupted
    }
}

struct Waiter {
    woken: AtomicBool,
    /// The table key this waiter is queued under; requeue changes it. Written under `TABLE`.
    key: AtomicUsize,
    // One queue per waiter, so wake-n and requeue move `Arc`s: OSTD cannot move a waker between
    // wait queues.
    wq: WaitQueue,
}

/// Holds the frame so it cannot be freed and reused (aliasing a live key) while waiters exist.
struct Bucket {
    _frame: UFrame,
    waiters: VecDeque<Arc<Waiter>>,
}

// shortcut: one global lock, which is fine on one CPU; SMP wants hashed buckets.
static TABLE: SpinLock<BTreeMap<usize, Bucket>> = SpinLock::new(BTreeMap::new());

/// The frame behind `va` in `space`, after breaking copy-on-write, and the word's offset in it.
fn lookup(space: &VmSpace, t: &Thread, va: Vaddr) -> Result<(UFrame, usize), FutexError> {
    if !va.is_multiple_of(4) {
        return Err(FutexError::Inval);
    }
    // The address is user-supplied; the cursors below unwrap on a range outside user space.
    if !is_in_user_space(va, 4) {
        return Err(FutexError::Fault);
    }
    handle_cow_fault(space, t.agent(), va).map_err(|_| FutexError::Budget)?;
    let page = va & !(PAGE_SIZE - 1);
    let guard = disable_preempt();
    let mut c = space.cursor(&guard, &(page..page + PAGE_SIZE)).unwrap();
    match c.query().unwrap().1 {
        Some(VmQueriedItem::MappedRam { frame, .. }) => Ok(((*frame).clone(), va - page)),
        _ => Err(FutexError::Fault),
    }
}

fn pop(tab: &mut BTreeMap<usize, Bucket>, key: usize, n: usize) -> Vec<Arc<Waiter>> {
    let Some(b) = tab.get_mut(&key) else {
        return Vec::new();
    };
    let n = n.min(b.waiters.len());
    let out = b.waiters.drain(..n).collect();
    if b.waiters.is_empty() {
        tab.remove(&key);
    }
    out
}

fn mark_woken(ws: &[Arc<Waiter>]) {
    for w in ws {
        w.woken.store(true, Ordering::Release);
    }
}

fn wake_all(ws: &[Arc<Waiter>]) {
    for w in ws {
        w.wq.wake_all();
    }
}

/// Blocks while `*va == expected`, until woken or interrupted.
pub fn wait(t: &Thread, space: &VmSpace, va: Vaddr, expected: u32) -> Result<(), FutexError> {
    let (frame, off) = lookup(space, t, va)?;
    let key = frame.paddr() + off;
    let w = Arc::new(Waiter {
        woken: AtomicBool::new(false),
        key: AtomicUsize::new(key),
        wq: WaitQueue::new(),
    });
    {
        let mut tab = TABLE.lock();
        // Read through the frame, not the user pointer, so nothing can fault under the lock.
        let v: u32 = frame.read_val(off).map_err(|_| FutexError::Fault)?;
        if v != expected {
            return Err(FutexError::Again);
        }
        tab.entry(key)
            .or_insert_with(|| Bucket {
                _frame: frame,
                waiters: VecDeque::new(),
            })
            .waiters
            .push_back(w.clone());
    }
    match t.block_on(&w.wq, || w.woken.load(Ordering::Acquire).then_some(())) {
        Ok(()) => Ok(()),
        Err(Interrupted) => {
            let mut tab = TABLE.lock();
            // A wake that raced the interrupt wins, so it is not lost.
            if w.woken.load(Ordering::Acquire) {
                return Ok(());
            }
            let key = w.key.load(Ordering::Relaxed);
            if let Some(b) = tab.get_mut(&key) {
                b.waiters.retain(|x| !Arc::ptr_eq(x, &w));
                if b.waiters.is_empty() {
                    tab.remove(&key);
                }
            }
            Err(FutexError::Interrupted)
        }
    }
}

/// Wakes up to `n` waiters on `va`; returns how many.
pub fn wake(t: &Thread, space: &VmSpace, va: Vaddr, n: usize) -> Result<usize, FutexError> {
    let (frame, off) = lookup(space, t, va)?;
    let woken = {
        let mut tab = TABLE.lock();
        let ws = pop(&mut tab, frame.paddr() + off, n);
        mark_woken(&ws);
        ws
    };
    wake_all(&woken);
    Ok(woken.len())
}

/// Wakes up to `n_wake` waiters on `va`, then moves up to `n_move` of the rest to `va2`. With
/// `cmp`, fails with `Again` unless `*va == cmp`. Returns (woken, moved).
pub fn requeue(
    t: &Thread,
    space: &VmSpace,
    va: Vaddr,
    n_wake: usize,
    va2: Vaddr,
    n_move: usize,
    cmp: Option<u32>,
) -> Result<(usize, usize), FutexError> {
    let (f1, o1) = lookup(space, t, va)?;
    let (f2, o2) = lookup(space, t, va2)?;
    let (k1, k2) = (f1.paddr() + o1, f2.paddr() + o2);
    let (woken, moved) = {
        let mut tab = TABLE.lock();
        if let Some(c) = cmp {
            let v: u32 = f1.read_val(o1).map_err(|_| FutexError::Fault)?;
            if v != c {
                return Err(FutexError::Again);
            }
        }
        let woken = pop(&mut tab, k1, n_wake);
        mark_woken(&woken);
        let moved = if k1 == k2 {
            0
        } else {
            let ws = pop(&mut tab, k1, n_move);
            for w in &ws {
                w.key.store(k2, Ordering::Relaxed);
            }
            let n = ws.len();
            if n > 0 {
                tab.entry(k2)
                    .or_insert_with(|| Bucket {
                        _frame: f2,
                        waiters: VecDeque::new(),
                    })
                    .waiters
                    .extend(ws);
            }
            n
        };
        (woken, moved)
    };
    wake_all(&woken);
    Ok((woken.len(), moved))
}
