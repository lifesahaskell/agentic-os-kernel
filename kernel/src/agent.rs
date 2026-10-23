//! The agent object: a protection domain with a kernel-stamped credential, a root capability
//! space, a parent link, and a memory budget carved out of its parent's.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use ostd::sync::SpinLock;

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
    Cap(CapError),
}

impl From<CapError> for AgentError {
    fn from(e: CapError) -> Self {
        AgentError::Cap(e)
    }
}

pub struct Agent {
    cred: Credential,
    pub cspace: SpinLock<CSpace>,
    mem_limit_pages: usize,
    mem_used_pages: AtomicUsize,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl Agent {
    /// A root agent. Only the kernel's boot path creates these; everything else is a child.
    pub fn new_root(on_behalf_of: u64, mem_limit_pages: usize) -> Arc<Self> {
        Self::build(on_behalf_of, None, mem_limit_pages, CSpace::new())
    }

    fn build(on_behalf_of: u64, parent: Option<u64>, limit: usize, cspace: CSpace) -> Arc<Self> {
        let agent_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        Arc::new(Self {
            cred: Credential {
                agent_id,
                on_behalf_of,
                parent,
            },
            cspace: SpinLock::new(cspace),
            mem_limit_pages: limit,
            mem_used_pages: AtomicUsize::new(0),
        })
    }

    pub fn credential(&self) -> Credential {
        self.cred
    }

    pub fn mem_used_pages(&self) -> usize {
        self.mem_used_pages.load(Ordering::Relaxed)
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

    /// A child agent holding only attenuated copies of the listed parent slots, with a memory
    /// budget carved out of the parent's. The child inherits the human principal.
    // shortcut: carved budget is never returned to the parent, add on agent teardown.
    pub fn spawn_child(
        &self,
        grants: &[(usize, Rights)],
        mem_limit_pages: usize,
    ) -> Result<Arc<Self>, AgentError> {
        let mut cspace = CSpace::new();
        {
            let parent = self.cspace.lock();
            for &(slot, mask) in grants {
                let cap: Cap = parent.get(slot)?.derive(mask)?;
                cspace.insert(cap)?;
            }
        }
        self.charge_pages(mem_limit_pages)?;
        let c = &self.cred;
        Ok(Self::build(
            c.on_behalf_of,
            Some(c.agent_id),
            mem_limit_pages,
            cspace,
        ))
    }
}

#[cfg(ktest)]
mod test {
    use ostd::prelude::*;

    use super::*;
    use crate::{cap::Object, ipc::Endpoint};

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

        let child = parent.spawn_child(&[(send_slot, Rights::SEND)], 4).unwrap();

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
        assert_eq!(parent.spawn_child(&[], 7).err(), Some(AgentError::Budget));
        assert_eq!(parent.mem_used_pages(), 4, "failed carve charges nothing");
    }

    #[ktest]
    fn child_cannot_regrant_without_grant_right() {
        let parent = Agent::new_root(1, 10);
        let slot = parent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(Endpoint::new()), Rights::ALL))
            .unwrap();
        let child = parent.spawn_child(&[(slot, Rights::SEND)], 1).unwrap();
        assert_eq!(
            child.spawn_child(&[(0, Rights::SEND)], 0).err(),
            Some(AgentError::Cap(CapError::Rights))
        );
    }
}
