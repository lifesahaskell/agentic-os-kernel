//! The agent object: a protection domain with a kernel-stamped credential, a root capability
//! space, a parent link, and memory and CPU budgets carved out of its parent's.

use alloc::sync::{Arc, Weak};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use ostd::sync::{LocalIrqDisabled, SpinLock};

use crate::cap::{CSpace, Cap, CapError, Rights};

/// Stamped by the kernel on every IPC message. Senders cannot set it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Credential {
    pub agent_id: u64,
    /// The human principal this agent acts for.
    pub on_behalf_of: u64,
    pub parent: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AgentError {
    Budget,
    /// A frame allocation failed: the machine is out of memory, not the agent's budget.
    NoMemory,
    Cap(CapError),
    /// The agent is retiring or retired and takes no new space and no new child (D-02).
    Retired,
}

impl From<CapError> for AgentError {
    fn from(e: CapError) -> Self {
        AgentError::Cap(e)
    }
}

/// Something that pays for memory in pages: an agent, a space that records what it charged so
/// teardown can return it, or a private tally. The two halves are inverses.
pub trait Payer {
    /// Charges `pages`, or fails without charging.
    fn charge_pages(&self, pages: usize) -> Result<(), AgentError>;
    /// Returns `pages` previously charged.
    fn uncharge_pages(&self, pages: usize);
}

impl Payer for Agent {
    fn charge_pages(&self, pages: usize) -> Result<(), AgentError> {
        Agent::charge_pages(self, pages)
    }

    fn uncharge_pages(&self, pages: usize) {
        Agent::uncharge_pages(self, pages);
    }
}

impl<T: Payer + ?Sized> Payer for Arc<T> {
    fn charge_pages(&self, pages: usize) -> Result<(), AgentError> {
        (**self).charge_pages(pages)
    }

    fn uncharge_pages(&self, pages: usize) {
        (**self).uncharge_pages(pages);
    }
}

/// CPU budgets are timer ticks per period of this many ticks (10 ms at OSTD's 1000 Hz timer).
pub const CPU_PERIOD_TICKS: u64 = 10;

#[derive(Default)]
struct CpuUse {
    period: u64,
    used: u64,
    total: u64,
}

/// Where an agent is in its life. `Retiring` begins when the last counted Space is gone (D-02) and
/// lasts until the agent's own children are gone too; `Retired` is the end, and the carve is back
/// with the parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Live,
    Retiring,
    Retired,
}

/// The retirement ledger of one agent.
///
/// Lock rule: `life` is taken before `cspace`, and no code holds `life` while it drops
/// capabilities or calls into the parent. Both can come back into this agent or the parent's
/// `life` (a dropped capability can be the last reference to a Space or an Agent).
struct Life {
    stage: Stage,
    /// Counted Spaces that exist (`enter_space` raises it, `leave_space` lowers it).
    spaces: usize,
    /// Child agents whose carve this agent has not yet got back.
    children: usize,
    /// Set under the lock by whoever sends the carve back, so it goes back at most once.
    carve_returned: bool,
}

pub struct Agent {
    cred: Credential,
    pub cspace: SpinLock<CSpace>,
    mem_limit_pages: usize,
    mem_used_pages: AtomicUsize,
    cpu_quota_ticks: u64,
    /// Ticks per period handed to children; the agent's own threads get the rest.
    cpu_carved_ticks: AtomicU64,
    // IRQ-safe: charged from the timer interrupt.
    cpu: SpinLock<CpuUse, LocalIrqDisabled>,
    life: SpinLock<Life>,
    /// The agent that carved this one's budgets. Weak, so a child never keeps its parent alive.
    parent: Option<Weak<Agent>>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Agents that exist: raised by `build`, lowered when one is dropped.
static LIVE_AGENTS: AtomicUsize = AtomicUsize::new(0);

/// Agents that exist now, for the leak checks.
pub fn live_agents() -> usize {
    LIVE_AGENTS.load(Ordering::Acquire)
}

impl Agent {
    /// A root agent with a whole CPU. Only the kernel's boot path creates these; everything
    /// else is a child.
    pub fn new_root(on_behalf_of: u64, mem_limit_pages: usize) -> Arc<Self> {
        Self::build(
            on_behalf_of,
            None,
            None,
            mem_limit_pages,
            CPU_PERIOD_TICKS,
            CSpace::new(),
        )
    }

    fn build(
        on_behalf_of: u64,
        parent: Option<u64>,
        parent_link: Option<Weak<Agent>>,
        limit: usize,
        cpu_quota_ticks: u64,
        cspace: CSpace,
    ) -> Arc<Self> {
        let agent_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        LIVE_AGENTS.fetch_add(1, Ordering::AcqRel);
        Arc::new(Self {
            cred: Credential {
                agent_id,
                on_behalf_of,
                parent,
            },
            cspace: SpinLock::new(cspace),
            mem_limit_pages: limit,
            mem_used_pages: AtomicUsize::new(0),
            cpu_quota_ticks,
            cpu_carved_ticks: AtomicU64::new(0),
            cpu: SpinLock::new(CpuUse::default()),
            life: SpinLock::new(Life {
                stage: Stage::Live,
                spaces: 0,
                children: 0,
                carve_returned: false,
            }),
            parent: parent_link,
        })
    }

    pub fn credential(&self) -> Credential {
        self.cred
    }

    pub fn mem_used_pages(&self) -> usize {
        self.mem_used_pages.load(Ordering::Relaxed)
    }

    /// Ticks per period this agent has carved out for its children.
    pub fn cpu_carved_ticks(&self) -> u64 {
        self.cpu_carved_ticks.load(Ordering::Relaxed)
    }

    /// True from the moment retirement begins, whether or not the carve is back yet.
    pub fn is_retired(&self) -> bool {
        self.life.lock().stage != Stage::Live
    }

    /// Charges `pages` against the memory budget, or fails without charging.
    pub fn charge_pages(&self, pages: usize) -> Result<(), AgentError> {
        self.mem_used_pages
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(pages)
                    .filter(|&n| n <= self.mem_limit_pages)
            })
            .map(drop)
            .map_err(|_| AgentError::Budget)
    }

    /// Returns `pages` to the memory budget, the inverse of `charge_pages`. Saturates at 0; a
    /// refund larger than the charge is a bug in the caller and trips a debug assertion.
    pub fn uncharge_pages(&self, pages: usize) {
        let before = self
            .mem_used_pages
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                Some(used.saturating_sub(pages))
            })
            .unwrap_or_default();
        debug_assert!(before >= pages, "uncharge_pages underflow");
    }

    /// Whether this agent's threads may run in the period containing tick `now`.
    pub fn has_cpu(&self, now: u64) -> bool {
        let c = self.cpu.lock();
        let used = if c.period == now / CPU_PERIOD_TICKS {
            c.used
        } else {
            0
        };
        used < self.cpu_quota_ticks - self.cpu_carved_ticks.load(Ordering::Relaxed)
    }

    /// Charges the tick at `now` to this agent.
    // shortcut: sampled at the tick, so a thread that runs between ticks is not charged; use
    // TSC-based accounting when budgets need sub-tick precision.
    pub fn charge_tick(&self, now: u64) {
        let mut c = self.cpu.lock();
        let period = now / CPU_PERIOD_TICKS;
        if c.period != period {
            c.period = period;
            c.used = 0;
        }
        c.used += 1;
        c.total += 1;
    }

    /// Ticks charged to this agent since it was created.
    pub fn cpu_used_ticks(&self) -> u64 {
        self.cpu.lock().total
    }

    /// A child agent holding only attenuated copies of the listed parent slots, with memory and
    /// CPU budgets carved out of the parent's. The child inherits the human principal. The carve
    /// comes back when the child retires (see `retire`); `Retired` once this agent is retiring.
    pub fn spawn_child(
        self: &Arc<Self>,
        grants: &[(usize, Rights)],
        mem_limit_pages: usize,
        cpu_quota_ticks: u64,
    ) -> Result<Arc<Self>, AgentError> {
        {
            let mut life = self.life.lock();
            if life.stage != Stage::Live {
                return Err(AgentError::Retired);
            }
            life.children += 1;
        }
        let child = self.carve_child(grants, mem_limit_pages, cpu_quota_ticks);
        if child.is_err() {
            self.child_gone();
        }
        child
    }

    /// The part of `spawn_child` after the child is counted: derive, carve, charge.
    fn carve_child(
        self: &Arc<Self>,
        grants: &[(usize, Rights)],
        mem_limit_pages: usize,
        cpu_quota_ticks: u64,
    ) -> Result<Arc<Self>, AgentError> {
        let mut cspace = CSpace::new();
        {
            let parent = self.cspace.lock();
            for &(slot, mask) in grants {
                let cap: Cap = parent.get(slot)?.derive(mask)?;
                cspace.insert(cap)?;
            }
        }
        self.cpu_carved_ticks
            .try_update(Ordering::AcqRel, Ordering::Acquire, |carved| {
                carved
                    .checked_add(cpu_quota_ticks)
                    .filter(|&n| n <= self.cpu_quota_ticks)
            })
            .map_err(|_| AgentError::Budget)?;
        if let Err(e) = self.charge_pages(mem_limit_pages) {
            self.cpu_carved_ticks
                .fetch_sub(cpu_quota_ticks, Ordering::AcqRel);
            return Err(e);
        }
        let c = &self.cred;
        Ok(Self::build(
            c.on_behalf_of,
            Some(c.agent_id),
            Some(Arc::downgrade(self)),
            mem_limit_pages,
            cpu_quota_ticks,
            cspace,
        ))
    }

    /// Counts one more Space of this agent, or `Retired` when the agent has begun to retire.
    pub fn enter_space(&self) -> Result<(), AgentError> {
        let mut life = self.life.lock();
        if life.stage != Stage::Live {
            return Err(AgentError::Retired);
        }
        life.spaces += 1;
        Ok(())
    }

    /// One counted Space is gone. When it was the last and this is a child agent, retirement
    /// begins (D-02). A root agent never retires on its own.
    pub fn leave_space(&self) {
        let begin = {
            let mut life = self.life.lock();
            debug_assert!(life.spaces > 0, "leave_space without enter_space");
            life.spaces = life.spaces.saturating_sub(1);
            let last = life.spaces == 0 && life.stage == Stage::Live && self.parent.is_some();
            if last {
                life.stage = Stage::Retiring;
            }
            last
        };
        if begin {
            self.retire();
        }
    }

    /// Retirement, begun by `leave_space` at the last counted Space (D-01, D-02): the CSpace is
    /// drained and closed at once, then the carve goes back to the parent once this agent's own
    /// children are gone (D-08).
    ///
    /// Capabilities this agent derived to others are not touched: transfer is `derive`, a copy, so
    /// the kernel revokes nothing here and the copies survive (D-01).
    ///
    /// Known gaps:
    /// - pages still staged in messages a retired agent sent are counted against nobody until
    ///   those messages are consumed;
    /// - procmgr's exec must build the new space before the old one is torn down, or the agent
    ///   retires between the two (D-02's stated consequence);
    /// - an agent dropped while it still has children never returns its carve: a leak toward its
    ///   parent, never a double spend;
    /// - a retiring owner refused by `Space::from_clone` is reported as `CloneError::Budget`.
    fn retire(&self) {
        let caps = {
            let mut cspace = self.cspace.lock();
            let caps = cspace.drain();
            cspace.close();
            caps
        };
        // Outside the lock: dropping one can drop a Space or an Agent, and that can come back
        // here through `child_gone`.
        drop(caps);
        self.finish_if_childless();
    }

    /// Ends a retirement that has no children left to wait for, and sends the carve back. The
    /// stage and the flag change under one lock hold, so whoever gets here first sends it and
    /// nobody sends it twice.
    fn finish_if_childless(&self) {
        let done = {
            let mut life = self.life.lock();
            let done = life.stage == Stage::Retiring && life.children == 0 && !life.carve_returned;
            if done {
                life.stage = Stage::Retired;
                life.carve_returned = true;
            }
            done
        };
        if done {
            self.return_carve();
        }
    }

    /// Gives this agent's carve back to the parent and tells it one child is gone. A parent that
    /// no longer exists has nothing to take it.
    fn return_carve(&self) {
        let Some(parent) = self.parent.as_ref().and_then(Weak::upgrade) else {
            return;
        };
        parent.uncharge_pages(self.mem_limit_pages);
        parent
            .cpu_carved_ticks
            .fetch_sub(self.cpu_quota_ticks, Ordering::AcqRel);
        parent.child_gone();
    }

    /// A child's carve is back. When this agent was waiting only for its last child, its own
    /// retirement finishes now, and its carve goes one level further up (the D-08 cascade).
    fn child_gone(&self) {
        {
            let mut life = self.life.lock();
            life.children = life.children.saturating_sub(1);
        }
        self.finish_if_childless();
    }
}

impl Drop for Agent {
    /// A child that never had a counted Space still returns its budget.
    fn drop(&mut self) {
        LIVE_AGENTS.fetch_sub(1, Ordering::AcqRel);
        let give_back = {
            let mut life = self.life.lock();
            let give = self.parent.is_some() && !life.carve_returned && life.children == 0;
            if give {
                life.carve_returned = true;
            }
            give
        };
        if give_back {
            self.return_carve();
        }
    }
}

#[cfg(ktest)]
mod test {
    use alloc::vec::Vec;

    use ostd::{
        prelude::*,
        sync::SpinLock,
        task::{Task, TaskOptions},
    };

    use super::*;
    use crate::{
        cap::Object,
        ipc::Endpoint,
        thread::Thread,
        user::{
            Exit, load_image,
            testkit::{join, spawn_image},
            user_image,
        },
    };

    #[ktest]
    fn child_gets_attenuated_subset_and_carved_budget() {
        let parent = Agent::new_root(1000, 10);
        let (send_slot, other_slot) = {
            let mut cs = parent.cspace.lock();
            let ep = Object::Endpoint(Endpoint::new());
            (
                cs.insert(Cap::new(ep.clone(), Rights::ALL)).unwrap(),
                cs.insert(Cap::new(ep, Rights::ALL)).unwrap(),
            )
        };

        let child = parent
            .spawn_child(&[(send_slot, Rights::SEND)], 4, 2)
            .unwrap();

        let cred = child.credential();
        assert_eq!(cred.on_behalf_of, 1000);
        assert_eq!(cred.parent, Some(parent.credential().agent_id));
        assert_ne!(cred.agent_id, parent.credential().agent_id);
        let cs = child.cspace.lock();
        assert_eq!(cs.get(0).unwrap().rights(), Rights::SEND);
        assert_eq!(
            cs.get(1).err(),
            Some(CapError::NoSlot),
            "only listed slots are copied"
        );
        drop(cs);
        let _ = other_slot;

        assert_eq!(parent.mem_used_pages(), 4);
        assert_eq!(
            parent.spawn_child(&[], 7, 0).err(),
            Some(AgentError::Budget)
        );
        assert_eq!(parent.mem_used_pages(), 4, "failed carve charges nothing");

        // 2 of 10 CPU ticks are carved. A memory failure must hand the CPU carve back.
        assert_eq!(
            parent.spawn_child(&[], 0, 9).err(),
            Some(AgentError::Budget)
        );
        assert_eq!(
            parent.spawn_child(&[], 7, 1).err(),
            Some(AgentError::Budget)
        );
        // Held: a child dropped without ever running a process returns its carve at once.
        let _carved = parent.spawn_child(&[], 0, 8).unwrap();
        assert!(
            !parent.has_cpu(0),
            "all 10 ticks carved; nothing left for the parent"
        );
    }

    #[ktest]
    fn child_cannot_regrant_without_grant_right() {
        let parent = Agent::new_root(1, 10);
        let slot = parent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(Endpoint::new()), Rights::ALL))
            .unwrap();
        let child = parent.spawn_child(&[(slot, Rights::SEND)], 1, 1).unwrap();
        assert_eq!(
            child.spawn_child(&[(0, Rights::SEND)], 0, 0).err(),
            Some(AgentError::Cap(CapError::Rights))
        );
    }

    /// What kernel client calls returned, in order: (client tag, reply `words[0]`).
    type Answers = Arc<SpinLock<Vec<(u64, u64)>>>;

    /// Spawns a kernel task that calls `ep` once per label as `t`, logging each reply.
    fn spawn_calls(ep: &Arc<Endpoint>, t: &Arc<Thread>, labels: &[u64], log: &Answers) {
        let (ep, t, labels, log) = (ep.clone(), t.clone(), labels.to_vec(), log.clone());
        TaskOptions::new(move || {
            for label in labels {
                let words = ep.call(&t, label, [0; 4]).unwrap();
                log.lock().push((t.tag(), words[0]));
            }
        })
        .spawn()
        .unwrap();
    }

    fn wait_answers(log: &Answers, n: usize) {
        while log.lock().len() < n {
            Task::yield_now();
        }
    }

    /// AGNT-05: a ring-3 server keyed on the kernel-stamped tag answers the fifth request of one
    /// tag in a window with EAGAIN (limit 4) and still serves a second tag.
    #[ktest]
    fn request_ledger_limit_answers_eagain_per_tag() {
        let ep = Endpoint::new();
        let server_agent = Agent::new_root(42, 64);
        let slot = server_agent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(ep.clone()), Rights::ALL))
            .unwrap();
        assert_eq!(slot, 0);
        let client_agent = Agent::new_root(43, 8);
        let img = load_image(user_image!("ledger_srv")).unwrap();
        let exit = spawn_image(&Thread::new(server_agent), &img, 0);

        let (a, b) = (
            Thread::new_tagged(client_agent.clone(), 0xA),
            Thread::new_tagged(client_agent, 0xB),
        );
        let log: Answers = Arc::new(SpinLock::new(Vec::new()));
        let eagain = (-abi::errno::EAGAIN) as u64;

        spawn_calls(&ep, &a, &[1; 5], &log);
        wait_answers(&log, 5);
        assert_eq!(
            *log.lock(),
            [(0xA, 0), (0xA, 0), (0xA, 0), (0xA, 0), (0xA, eagain)]
        );

        // The second tag is served in full after the first was refused.
        spawn_calls(&ep, &b, &[1; 4], &log);
        wait_answers(&log, 9);
        assert!(log.lock()[5..].iter().all(|&r| r == (0xB, 0)));

        // The first tag stays refused within the window.
        spawn_calls(&ep, &a, &[1], &log);
        wait_answers(&log, 10);
        assert_eq!(log.lock()[9], (0xA, eagain));

        // The stop label is answered and ends the server.
        spawn_calls(&ep, &b, &[0xFFFF], &log);
        assert_eq!(join(&exit), Exit::Code(0));
        wait_answers(&log, 11);
        assert_eq!(log.lock()[10], (0xB, 0));
    }

    /// The ledger's window, recycling and table-full behaviour, run in ring 3 against synthetic
    /// times: `ledger_srv` exits 0 only if every check holds, or with the number of the first
    /// check that failed.
    #[ktest]
    fn request_ledger_self_test() {
        let img = load_image(user_image!("ledger_srv")).unwrap();
        let t = Thread::new(Agent::new_root(44, 8));
        assert_eq!(join(&spawn_image(&t, &img, 1)), Exit::Code(0));
    }
}
