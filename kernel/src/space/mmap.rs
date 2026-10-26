//! The self-memory calls on a `Space`: `mmap`, `munmap`, `mprotect` and `brk` act on the caller's
//! own address space and are charged through the space's ledger to its owner agent (AGNT-02,
//! OD-02).
//!
//! Anonymous private mappings are populated eagerly. Every page is allocated and charged at the
//! call, so a mapping the budget cannot pay for fails there with `ENOMEM`, maps nothing and
//! charges nothing, instead of failing at a later fault.
//!
//! One lock, `Space::mm`, orders every call and teardown. It is taken before the `disable_preempt`
//! guard that a page-table cursor needs, never after. Each helper below makes its own cursor and
//! drops it before returning: two live cursors over overlapping ranges would deadlock.

use alloc::{collections::BTreeMap, vec::Vec};
use core::ops::Range;

use abi::{MAP_FIXED, MMAP_BASE, MMAP_TOP, PROT_EXEC, PROT_NONE, PROT_READ, PROT_WRITE, USER_TOP};
use ostd::{
    mm::{
        CachePolicy, FrameAllocOptions, PAGE_SIZE, PageFlags, PageProperty, Vaddr, VmSpace,
        tlb::TlbFlushOp, vm_space::VmQueriedItem,
    },
    task::disable_preempt,
};

use super::Space;
use crate::{agent::Payer, syscall::Errno, vm::COW};

/// Most mapping records one space holds. A call that would add one past it is `ENOMEM`, Linux's
/// `max_map_count` result; a `munmap` that splits a record counts as adding one. Bounds the kernel
/// heap a `PROT_NONE` reservation, which costs no pages, can take.
pub const MAX_MAPS_PER_SPACE: usize = 1024;

/// One mapping a call made: `[start, end)` where `start` is the key, with the protection it was
/// last given. A `PROT_NONE` record may cover pages that were never populated.
#[derive(Clone, Copy, Debug)]
struct Rec {
    end: Vaddr,
    prot: usize,
}

/// A space's mapping records. The image, the stack and the brk area are not records: they sit in
/// the page table only.
#[derive(Default)]
pub struct Maps {
    recs: BTreeMap<Vaddr, Rec>,
    #[allow(dead_code, reason = "read by brk, which lands in a later task")]
    brk_base: Vaddr,
    #[allow(dead_code, reason = "read by brk, which lands in a later task")]
    brk_cur: Vaddr,
}

impl Maps {
    /// Forgets every record: the space is gone.
    pub(super) fn clear(&mut self) {
        self.recs.clear();
    }
}

/// The page flags `prot` grants. `PROT_WRITE` or `PROT_EXEC` implies read, as on Linux x86-64.
fn prop_flags(prot: usize) -> PageFlags {
    let mut f = PageFlags::empty();
    if prot & (PROT_READ | PROT_WRITE | PROT_EXEC) != 0 {
        f |= PageFlags::R;
    }
    if prot & PROT_WRITE != 0 {
        f |= PageFlags::W;
    }
    if prot & PROT_EXEC != 0 {
        f |= PageFlags::X;
    }
    f
}

/// The first page mapped in the page table inside `range`, if any.
fn first_mapped(vm: &VmSpace, range: &Range<Vaddr>) -> Option<Vaddr> {
    let guard = disable_preempt();
    let mut c = vm.cursor(&guard, range).ok()?;
    c.find_next(range.len())
}

/// The end of the record that overlaps `range`, if one does. Records never overlap each other, so
/// the last one that starts below `range.end` has the greatest end of any that could.
fn record_overlap(recs: &BTreeMap<Vaddr, Rec>, range: &Range<Vaddr>) -> Option<Vaddr> {
    recs.range(..range.end)
        .next_back()
        .map(|(_, r)| r.end)
        .filter(|&end| end > range.start)
}

/// Whether nothing occupies `range`: no record overlaps it and no page is mapped in it. The page
/// table walk covers what no record names (the image, the stack, the brk area, pages a clone
/// copied in), and must run before every `map`, because OSTD's `map` panics on an occupied slot.
fn is_free(vm: &VmSpace, recs: &BTreeMap<Vaddr, Rec>, range: &Range<Vaddr>) -> bool {
    record_overlap(recs, range).is_none() && first_mapped(vm, range).is_none()
}

/// A free range of `size` bytes: the hint `addr` when it is page-aligned, inside
/// `[PAGE_SIZE, MMAP_TOP)` and free for the whole length, otherwise the first free gap in
/// `[MMAP_BASE, MMAP_TOP)`.
fn place(
    vm: &VmSpace,
    recs: &BTreeMap<Vaddr, Rec>,
    hint: Vaddr,
    size: usize,
) -> Result<Vaddr, Errno> {
    if hint.is_multiple_of(PAGE_SIZE)
        && hint >= PAGE_SIZE
        && let Some(end) = hint.checked_add(size)
        && end <= MMAP_TOP
        && is_free(vm, recs, &(hint..end))
    {
        return Ok(hint);
    }
    let mut cand = MMAP_BASE;
    loop {
        let end = cand.checked_add(size).filter(|&e| e <= MMAP_TOP);
        let Some(end) = end else {
            return Err(Errno::ENOMEM);
        };
        let range = cand..end;
        // Skip past whatever is in the way and look again.
        if let Some(rec_end) = record_overlap(recs, &range) {
            cand = rec_end;
        } else if let Some(va) = first_mapped(vm, &range) {
            cand = va + PAGE_SIZE;
        } else {
            return Ok(cand);
        }
    }
}

/// The records that overlap `range`, in address order.
fn overlapping(recs: &BTreeMap<Vaddr, Rec>, range: &Range<Vaddr>) -> Vec<(Vaddr, Rec)> {
    let mut v: Vec<_> = recs
        .range(..range.end)
        .rev()
        .take_while(|(_, r)| r.end > range.start)
        .map(|(s, r)| (*s, *r))
        .collect();
    v.reverse();
    v
}

/// How many records exist once `range` is cut out of them: each overlapping record is replaced by
/// what is left of it outside `range`, plus its part inside when `keep_inside` (`mprotect`
/// reprotects it; `munmap` drops it). Only the first and last overlapping records can leave a
/// remainder.
fn len_after_cut(recs: &BTreeMap<Vaddr, Rec>, range: &Range<Vaddr>, keep_inside: bool) -> usize {
    let over = overlapping(recs, range);
    let left = over.first().is_some_and(|(s, _)| *s < range.start);
    let right = over.last().is_some_and(|(_, r)| r.end > range.end);
    let inside = if keep_inside { over.len() } else { 0 };
    recs.len() - over.len() + inside + usize::from(left) + usize::from(right)
}

/// Cuts `range` out of the records: what lies outside it stays, with its old protection; what lies
/// inside is dropped, or given `prot` when one is passed. The caller checked `len_after_cut`.
fn cut(recs: &mut BTreeMap<Vaddr, Rec>, range: &Range<Vaddr>, prot: Option<usize>) {
    for (start, rec) in overlapping(recs, range) {
        recs.remove(&start);
        if start < range.start {
            recs.insert(
                start,
                Rec {
                    end: range.start,
                    prot: rec.prot,
                },
            );
        }
        if rec.end > range.end {
            recs.insert(
                range.end,
                Rec {
                    end: rec.end,
                    prot: rec.prot,
                },
            );
        }
        if let Some(prot) = prot {
            recs.insert(
                start.max(range.start),
                Rec {
                    end: rec.end.min(range.end),
                    prot,
                },
            );
        }
    }
}

/// `range` split into the parts a record covers (`true`) and the parts none does (`false`).
fn segments(recs: &BTreeMap<Vaddr, Rec>, range: &Range<Vaddr>) -> Vec<(Range<Vaddr>, bool)> {
    let mut out = Vec::new();
    let mut at = range.start;
    for (start, rec) in overlapping(recs, range) {
        let (s, e) = (start.max(range.start), rec.end.min(range.end));
        if s > at {
            out.push((at..s, false));
        }
        out.push((s..e, true));
        at = e;
    }
    if at < range.end {
        out.push((at..range.end, false));
    }
    out
}

/// The maximal runs of `range` where no page is mapped in the page table.
fn holes(vm: &VmSpace, range: &Range<Vaddr>) -> Vec<Range<Vaddr>> {
    let mut out = Vec::new();
    let mut cur = range.start;
    while cur < range.end {
        match first_mapped(vm, &(cur..range.end)) {
            Some(va) => {
                if va > cur {
                    out.push(cur..va);
                }
                cur = va + PAGE_SIZE;
            }
            None => {
                out.push(cur..range.end);
                break;
            }
        }
    }
    out
}

fn pages_in(runs: &[Range<Vaddr>]) -> usize {
    runs.iter().map(|r| r.len() / PAGE_SIZE).sum()
}

impl Space {
    /// Maps `pages` zeroed frames at the cursor's position with `prop`. Returns how many it
    /// mapped: fewer than `pages` when the frame allocator ran dry.
    fn map_zeroed(
        c: &mut ostd::mm::vm_space::CursorMut<'_>,
        pages: usize,
        prop: PageProperty,
    ) -> usize {
        for done in 0..pages {
            let Ok(frame) = FrameAllocOptions::new().alloc_frame() else {
                return done;
            };
            c.map(frame.into(), prop);
        }
        pages
    }

    /// Allocates and maps a zeroed frame for each page of `range` with `flags`, charging nothing.
    /// The range must hold no mapped page. A failed allocation unmaps what this call mapped and is
    /// `ENOMEM`.
    fn map_pages(&self, range: &Range<Vaddr>, flags: PageFlags) -> Result<(), Errno> {
        let pages = range.len() / PAGE_SIZE;
        let guard = disable_preempt();
        let Ok(mut c) = self.vm.cursor_mut(&guard, range) else {
            return Err(Errno::ENOMEM);
        };
        let prop = PageProperty::new_user(flags, CachePolicy::Writeback);
        let done = Self::map_zeroed(&mut c, pages, prop);
        if done < pages {
            if done > 0 && c.jump(range.start).is_ok() {
                // `unmap` flushes the TLB itself.
                c.unmap(done * PAGE_SIZE);
            }
            return Err(Errno::ENOMEM);
        }
        Ok(())
    }

    /// Charges `range`'s pages in one call, then maps a zeroed frame for each with `flags`. A
    /// refused charge is `ENOMEM` before any frame exists; a failed allocation refunds the whole
    /// count. Either way nothing is left behind.
    fn populate(&self, range: Range<Vaddr>, flags: PageFlags) -> Result<(), Errno> {
        let pages = range.len() / PAGE_SIZE;
        self.charge_pages(pages).map_err(|_| Errno::ENOMEM)?;
        self.map_pages(&range, flags).inspect_err(|_| {
            self.uncharge_pages(pages);
        })
    }

    /// Unmaps every page in `range`, whatever mapped it, and refunds them. Returns the count.
    fn unmap_range(&self, range: &Range<Vaddr>) -> usize {
        let guard = disable_preempt();
        let Ok(mut c) = self.vm.cursor_mut(&guard, range) else {
            return 0;
        };
        let n = c.unmap(range.len());
        drop(c);
        if n > 0 {
            self.uncharge_pages(n);
        }
        n
    }

    /// Changes the protection of every page mapped in `range`. A page whose frame another
    /// mapping shares gets `COW` in place of `W` (the rule `vm::cow_clone` keeps).
    fn protect_range(&self, range: &Range<Vaddr>, prot: usize) {
        let guard = disable_preempt();
        let Ok(mut c) = self.vm.cursor_mut(&guard, range) else {
            return;
        };
        let want = prop_flags(prot);
        while c.virt_addr() < range.end {
            if c.find_next(range.end - c.virt_addr()).is_none() {
                break;
            }
            let shared = matches!(
                c.query(),
                Ok((_, Some(VmQueriedItem::MappedRam { frame, .. }))) if frame.reference_count() > 1
            );
            let flags = if want.contains(PageFlags::W) && shared {
                (want - PageFlags::W) | COW
            } else {
                want
            };
            c.protect_next(PAGE_SIZE, |f, _| {
                *f = (*f - (PageFlags::R | PageFlags::W | PageFlags::X | COW)) | flags;
            });
        }
        c.flusher()
            .issue_tlb_flush(TlbFlushOp::for_range(range.clone()));
        c.flusher().dispatch_tlb_flush();
    }

    /// `mmap(addr, len, prot, flags, off)` for anonymous private memory in this space; the
    /// arguments were checked by `syscall::decode`. Returns the address.
    ///
    /// `ENOMEM`: the space is dying, the record limit is reached, no gap is free, or the budget
    /// or the frame allocator refused the pages. `MAP_FIXED` replaces whatever is there, as on
    /// Linux; if the new pages then cannot be had, the old ones stay gone.
    pub fn mmap(
        &self,
        addr: Vaddr,
        len: usize,
        prot: usize,
        flags: usize,
        _off: usize,
    ) -> Result<Vaddr, Errno> {
        if len == 0 || prot & !(PROT_READ | PROT_WRITE | PROT_EXEC) != 0 {
            return Err(Errno::EINVAL);
        }
        let size = len
            .checked_next_multiple_of(PAGE_SIZE)
            .ok_or(Errno::ENOMEM)?;
        let mut mm = self.mm.lock();
        // Under the lock teardown holds, so nothing is charged after its uncharge.
        if self.is_dying() {
            return Err(Errno::ENOMEM);
        }
        let (start, end) = if flags & MAP_FIXED != 0 {
            let end = addr.checked_add(size).ok_or(Errno::ENOMEM)?;
            if addr < PAGE_SIZE || end > USER_TOP {
                return Err(Errno::ENOMEM);
            }
            let range = addr..end;
            if len_after_cut(&mm.recs, &range, false) >= MAX_MAPS_PER_SPACE {
                return Err(Errno::ENOMEM);
            }
            self.unmap_range(&range);
            cut(&mut mm.recs, &range, None);
            (addr, end)
        } else {
            if mm.recs.len() >= MAX_MAPS_PER_SPACE {
                return Err(Errno::ENOMEM);
            }
            let start = place(&self.vm, &mm.recs, addr, size)?;
            (start, start + size)
        };
        if prot != PROT_NONE {
            self.populate(start..end, prop_flags(prot))?;
        }
        mm.recs.insert(start, Rec { end, prot });
        Ok(start)
    }

    /// `munmap(addr, len)`: unmaps every page in the range, whatever mapped it, refunds them and
    /// returns how many. A range with nothing mapped is `Ok(0)`, as on Linux.
    ///
    /// `EINVAL`: an unaligned address, `len` 0, or a range that leaves user space. `ENOMEM`: the
    /// space is dying, or a split would pass `MAX_MAPS_PER_SPACE`.
    pub fn munmap(&self, addr: Vaddr, len: usize) -> Result<usize, Errno> {
        if !addr.is_multiple_of(PAGE_SIZE) || len == 0 {
            return Err(Errno::EINVAL);
        }
        let end = len
            .checked_next_multiple_of(PAGE_SIZE)
            .and_then(|size| addr.checked_add(size))
            .filter(|&end| end <= USER_TOP)
            .ok_or(Errno::EINVAL)?;
        let range = addr..end;
        let mut mm = self.mm.lock();
        if self.is_dying() {
            return Err(Errno::ENOMEM);
        }
        if len_after_cut(&mm.recs, &range, false) > MAX_MAPS_PER_SPACE {
            return Err(Errno::ENOMEM);
        }
        let n = self.unmap_range(&range);
        cut(&mut mm.recs, &range, None);
        Ok(n)
    }

    /// `mprotect(addr, len, prot)`. Every page of the range must be mapped or inside a record.
    /// Pages of a record that were never populated (a `PROT_NONE` reservation) are charged and
    /// populated when `prot` grants access, and stay unpopulated for `PROT_NONE`.
    ///
    /// `EINVAL`: an unaligned address, unknown `prot` bits, or a range that leaves user space.
    /// `ENOMEM`: part of the range is neither mapped nor reserved, the space is dying, the record
    /// limit would be passed, or the budget or the frame allocator refused the pages. A refusal
    /// changes nothing.
    pub fn mprotect(&self, addr: Vaddr, len: usize, prot: usize) -> Result<(), Errno> {
        if !addr.is_multiple_of(PAGE_SIZE) || prot & !(PROT_READ | PROT_WRITE | PROT_EXEC) != 0 {
            return Err(Errno::EINVAL);
        }
        if len == 0 {
            return Ok(());
        }
        let end = len
            .checked_next_multiple_of(PAGE_SIZE)
            .and_then(|size| addr.checked_add(size))
            .filter(|&end| end <= USER_TOP)
            .ok_or(Errno::EINVAL)?;
        let range = addr..end;
        let mut mm = self.mm.lock();
        if self.is_dying() {
            return Err(Errno::ENOMEM);
        }
        // First pass: change nothing, find the pages a reservation still owes.
        let mut owed = Vec::new();
        for (seg, reserved) in segments(&mm.recs, &range) {
            let gaps = holes(&self.vm, &seg);
            if reserved {
                owed.extend(gaps);
            } else if !gaps.is_empty() {
                return Err(Errno::ENOMEM);
            }
        }
        if len_after_cut(&mm.recs, &range, true) > MAX_MAPS_PER_SPACE {
            return Err(Errno::ENOMEM);
        }
        // Second pass. Populate first, so a failure leaves the old protection in place.
        let missing = pages_in(&owed);
        if prot != PROT_NONE && missing > 0 {
            self.charge_pages(missing).map_err(|_| Errno::ENOMEM)?;
            let flags = prop_flags(prot);
            for (i, run) in owed.iter().enumerate() {
                if self.map_pages(run, flags).is_err() {
                    for done in &owed[..i] {
                        self.unmap_range(done);
                    }
                    self.uncharge_pages(missing - pages_in(&owed[..i]));
                    return Err(Errno::ENOMEM);
                }
            }
        }
        self.protect_range(&range, prot);
        cut(&mut mm.recs, &range, Some(prot));
        Ok(())
    }

    /// The protection the record covering `addr` last got, if one does.
    pub fn prot_at(&self, addr: Vaddr) -> Option<usize> {
        let mm = self.mm.lock();
        let (_, rec) = mm.recs.range(..=addr).next_back()?;
        (rec.end > addr).then_some(rec.prot)
    }

    /// Mapping records the space holds.
    pub fn map_count(&self) -> usize {
        self.mm.lock().recs.len()
    }
}

#[cfg(ktest)]
mod test {
    use abi::{MAP_ANONYMOUS, MAP_PRIVATE, USER_STACK_TOP, errno};
    use alloc::sync::Arc;
    use ostd::{
        mm::{VmWriter, io::util::HasVmReaderWriter, vm_space::VmQueriedItem},
        prelude::*,
    };

    use super::*;
    use crate::{
        agent::Agent,
        sched::install_space_switch,
        space::{Space, ThreadStart, test::wait_reaped},
        syscall::UserSlice,
        thread::Thread,
        user::{Exit, Image, load_image, testkit::join_thread, user_image},
    };

    const RW: usize = PROT_READ | PROT_WRITE;
    const ANON: usize = MAP_PRIVATE | MAP_ANONYMOUS;

    /// The image, an agent with `headroom` pages beyond the image's, and a space over them.
    fn world(name: &'static str, headroom: usize) -> (Arc<Agent>, Arc<Space>, Image) {
        let _ = name;
        install_space_switch();
        let img = load_image(user_image!("mmap")).unwrap();
        let a = Agent::new_root(7, img.pages + headroom);
        let s = Space::from_image(&img, a.clone()).unwrap();
        (a, s, img)
    }

    /// Whether any page is mapped in `range` of `vm`.
    fn any_mapped(vm: &VmSpace, range: Range<Vaddr>) -> bool {
        first_mapped(vm, &range).is_some()
    }

    /// Runs the `mmap` bin in `s` with `mode` and returns how its thread exited.
    fn run_ring3(a: &Arc<Agent>, s: &Arc<Space>, img: &Image, mode: u64) -> Exit {
        let th = Thread::new(a.clone());
        s.register(&th).unwrap();
        let start = ThreadStart {
            rip: img.entry as u64,
            rsp: USER_STACK_TOP as u64,
            arg: mode,
            fs_base: 0,
            clear_tid: 0,
        };
        crate::user::spawn_thread(s, th.clone(), start);
        join_thread(&th)
    }

    /// The property of the page at `va`, if it is mapped.
    fn prop_at(vm: &VmSpace, va: Vaddr) -> Option<PageProperty> {
        let guard = disable_preempt();
        let mut c = vm.cursor(&guard, &(va..va + PAGE_SIZE)).ok()?;
        match c.query() {
            Ok((_, Some(VmQueriedItem::MappedRam { prop, .. }))) => Some(prop),
            _ => None,
        }
    }

    /// The first word of the frame behind `va`, read through the frame and not the page table.
    fn frame_word(vm: &VmSpace, va: Vaddr) -> u32 {
        let guard = disable_preempt();
        let mut c = vm.cursor(&guard, &(va..va + PAGE_SIZE)).unwrap();
        let Ok((_, Some(VmQueriedItem::MappedRam { frame, .. }))) = c.query() else {
            panic!("no page at {va:#x}");
        };
        let mut b = [0u8; 4];
        frame.reader().read(&mut VmWriter::from(&mut b[..]));
        u32::from_le_bytes(b)
    }

    fn put(s: &Arc<Space>, a: &Arc<Agent>, va: Vaddr, w: u32) {
        crate::vm::copy_to_user(
            s.vm(),
            a.as_ref(),
            UserSlice::new(va, 4).unwrap(),
            &w.to_le_bytes(),
        )
        .unwrap();
    }

    fn get(s: &Arc<Space>, va: Vaddr) -> Result<u32, Errno> {
        let mut b = [0u8; 4];
        crate::vm::copy_from_user(s.vm(), &mut b, UserSlice::new(va, 4).unwrap())?;
        Ok(u32::from_le_bytes(b))
    }

    #[ktest]
    fn vmem_enomem_past_budget_from_ring3() {
        let (a, s, img) = world("ring3", 6);
        assert_eq!(
            run_ring3(&a, &s, &img, 1),
            Exit::Code(((errno::ENOMEM as usize) << 16) | 6)
        );
        wait_reaped(&s);
        assert_eq!(a.mem_used_pages(), 0, "teardown gave every page back");
    }

    #[ktest]
    fn vmem_enomem_maps_nothing() {
        let (a, s, _img) = world("nothing", 2);
        let used = a.mem_used_pages();
        assert_eq!(s.mmap(0, 4 * PAGE_SIZE, RW, ANON, 0), Err(Errno::ENOMEM));
        assert_eq!(a.mem_used_pages(), used);
        assert_eq!(s.charged(), used);
        assert_eq!(s.map_count(), 0);
        assert!(!any_mapped(s.vm(), MMAP_BASE..MMAP_BASE + 4 * PAGE_SIZE));
    }

    #[ktest]
    fn vmem_mmap_places_charges_and_zeroes() {
        let (a, s, _img) = world("places", 6);
        let used = a.mem_used_pages();
        let first = s.mmap(0, 3 * PAGE_SIZE, RW, ANON, 0).unwrap();
        let second = s.mmap(0, 3 * PAGE_SIZE, RW, ANON, 0).unwrap();
        assert_eq!(first, MMAP_BASE);
        assert_eq!(second, MMAP_BASE + 3 * PAGE_SIZE);
        assert_eq!(a.mem_used_pages(), used + 6);
        assert_eq!(s.map_count(), 2);
        s.vm().activate();
        let mut word = [0u8; 8];
        for page in 0..6 {
            let at = MMAP_BASE + page * PAGE_SIZE;
            for off in [0, PAGE_SIZE - 8] {
                crate::vm::copy_from_user(
                    s.vm(),
                    &mut word,
                    crate::syscall::UserSlice::new(at + off, 8).unwrap(),
                )
                .unwrap();
                assert_eq!(word, [0u8; 8], "page {page} offset {off} is not zero");
            }
        }
    }

    #[ktest]
    fn vmem_mmap_after_teardown_is_enomem() {
        let (a, s, _img) = world("dead", 6);
        s.kill();
        let used = a.mem_used_pages();
        assert_eq!(s.mmap(0, PAGE_SIZE, RW, ANON, 0), Err(Errno::ENOMEM));
        assert_eq!(a.mem_used_pages(), used);
        assert_eq!(s.map_count(), 0);
    }

    const NONE: usize = PROT_NONE;
    const FIXED_ANON: usize = ANON | MAP_FIXED;

    #[ktest]
    fn vmem_munmap_returns_pages_and_splits() {
        let (a, s, _img) = world("munmap", 8);
        let used = a.mem_used_pages();
        let base = s.mmap(0, 4 * PAGE_SIZE, RW, ANON, 0).unwrap();
        assert_eq!(a.mem_used_pages(), used + 4);
        assert_eq!(s.munmap(base + PAGE_SIZE, 2 * PAGE_SIZE), Ok(2));
        assert_eq!(a.mem_used_pages(), used + 2);
        assert_eq!(s.charged(), used + 2);
        assert_eq!(s.map_count(), 2);
        assert!(!any_mapped(s.vm(), base + PAGE_SIZE..base + 3 * PAGE_SIZE));
        assert!(any_mapped(s.vm(), base..base + PAGE_SIZE));
        assert!(any_mapped(
            s.vm(),
            base + 3 * PAGE_SIZE..base + 4 * PAGE_SIZE
        ));
        // A hole is a success that changes nothing.
        assert_eq!(s.munmap(base + PAGE_SIZE, 2 * PAGE_SIZE), Ok(0));
        assert_eq!(a.mem_used_pages(), used + 2);
        assert_eq!(s.munmap(base + 1, PAGE_SIZE), Err(Errno::EINVAL));
        assert_eq!(s.munmap(base, 0), Err(Errno::EINVAL));
        assert_eq!(a.mem_used_pages(), used + 2);
    }

    #[ktest]
    fn vmem_prot_none_reserves_without_charge() {
        let (a, s, _img) = world("none", 12);
        let used = a.mem_used_pages();
        let base = s.mmap(0, 8 * PAGE_SIZE, NONE, ANON, 0).unwrap();
        assert_eq!(a.mem_used_pages(), used, "a reservation costs no pages");
        assert!(!any_mapped(s.vm(), base..base + 8 * PAGE_SIZE));
        assert_eq!(s.prot_at(base), Some(PROT_NONE));
        assert_eq!(s.mprotect(base, 2 * PAGE_SIZE, RW), Ok(()));
        assert_eq!(s.prot_at(base), Some(RW));
        assert_eq!(s.prot_at(base + 2 * PAGE_SIZE), Some(PROT_NONE));
        assert_eq!(a.mem_used_pages(), used + 2);
        assert_eq!(s.mprotect(base, 8 * PAGE_SIZE, RW), Ok(()));
        assert_eq!(a.mem_used_pages(), used + 8, "six more, never eight");
        assert_eq!(s.charged(), used + 8);
    }

    #[ktest]
    fn vmem_mprotect_none_keeps_the_data() {
        let (a, s, _img) = world("keep", 4);
        let base = s.mmap(0, PAGE_SIZE, RW, ANON, 0).unwrap();
        let used = a.mem_used_pages();
        s.vm().activate();
        put(&s, &a, base, 0x55aa_55aa);
        assert_eq!(s.mprotect(base, PAGE_SIZE, NONE), Ok(()));
        assert_eq!(get(&s, base), Err(Errno::EFAULT));
        assert_eq!(s.mprotect(base, PAGE_SIZE, RW), Ok(()));
        assert_eq!(get(&s, base), Ok(0x55aa_55aa));
        assert_eq!(a.mem_used_pages(), used);
    }

    #[ktest]
    fn vmem_mprotect_write_on_a_shared_frame_sets_cow() {
        let (a, s, _img) = world("cow", 4);
        let base = s.mmap(0, PAGE_SIZE, RW, ANON, 0).unwrap();
        s.vm().activate();
        put(&s, &a, base, 0x1111);
        assert_eq!(s.mprotect(base, PAGE_SIZE, PROT_READ), Ok(()));
        let payer = Agent::new_root(8, 16);
        let child =
            crate::vm::cow_clone(s.vm(), base..base + PAGE_SIZE, payer.as_ref()).expect("clone");
        assert_eq!(s.mprotect(base, PAGE_SIZE, RW), Ok(()));
        let flags = prop_at(s.vm(), base).unwrap().flags;
        assert!(flags.contains(crate::vm::COW), "shared and writable: COW");
        assert!(!flags.contains(PageFlags::W), "shared: not writable yet");
        put(&s, &a, base, 0x2222);
        assert_eq!(get(&s, base), Ok(0x2222));
        assert_eq!(
            frame_word(&child, base),
            0x1111,
            "the child's frame is untouched"
        );
    }

    #[ktest]
    fn vmem_mprotect_unmapped_range_is_enomem() {
        let (a, s, _img) = world("hole", 4);
        let base = s.mmap(0, PAGE_SIZE, RW, ANON, 0).unwrap();
        let used = a.mem_used_pages();
        assert_eq!(
            s.mprotect(base, 2 * PAGE_SIZE, PROT_READ),
            Err(Errno::ENOMEM)
        );
        let flags = prop_at(s.vm(), base).unwrap().flags;
        assert!(flags.contains(PageFlags::W), "nothing changed");
        assert_eq!(s.prot_at(base), Some(RW));
        assert_eq!(a.mem_used_pages(), used);
        assert_eq!(s.map_count(), 1);
    }

    #[ktest]
    fn vmem_map_fixed_replaces() {
        let (a, s, _img) = world("fixed", 6);
        let base = s.mmap(0, 2 * PAGE_SIZE, RW, ANON, 0).unwrap();
        let used = a.mem_used_pages();
        s.vm().activate();
        put(&s, &a, base, 0xdead_beef);
        assert_eq!(s.mmap(base, 2 * PAGE_SIZE, RW, FIXED_ANON, 0), Ok(base));
        assert_eq!(a.mem_used_pages(), used, "net unchanged");
        assert_eq!(s.map_count(), 1);
        assert_eq!(get(&s, base), Ok(0));
        assert_eq!(get(&s, base + PAGE_SIZE), Ok(0));
    }

    #[ktest]
    fn vmem_ring3_protect_and_unmap() {
        let (a, s, img) = world("ring3-prot", 8);
        assert_eq!(run_ring3(&a, &s, &img, 2), Exit::Code(0));
        wait_reaped(&s);
        assert_eq!(a.mem_used_pages(), 0);
    }
}
