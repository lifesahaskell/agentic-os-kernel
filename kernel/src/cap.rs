//! Capability space: rights-masked handles, attenuating derive, and per-grant revocation through
//! forwarders (EROS/Coyotos style, see docs/architecture/option-b-microkernel.md section 2.2).

use alloc::{sync::Arc, vec::Vec};

use ostd::sync::SpinLock;

use crate::{agent::Agent, ipc::Endpoint};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rights(u32);

impl Rights {
    pub const NONE: Self = Self(0);
    pub const SEND: Self = Self(1);
    pub const RECV: Self = Self(2);
    /// May derive or grant copies of this capability.
    pub const GRANT: Self = Self(4);
    pub const ALL: Self = Self(7);

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    const fn mask(self, m: Self) -> Self {
        Self(self.0 & m.0)
    }
}

#[derive(Clone)]
pub enum Object {
    Endpoint(Arc<Endpoint>),
    Agent(Arc<Agent>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum CapError {
    NoSlot,
    Full,
    Rights,
    Revoked,
}

#[derive(Clone)]
enum Target {
    Direct(Object),
    Via(Arc<Forwarder>),
}

/// One revocable grant. Revoking it kills every capability derived through it, including
/// grants chained behind it, and nothing else.
pub struct Forwarder {
    target: SpinLock<Option<Target>>,
}

impl Forwarder {
    pub fn revoke(&self) {
        *self.target.lock() = None;
    }
}

impl Target {
    fn resolve(&self) -> Result<Object, CapError> {
        match self {
            Target::Direct(o) => Ok(o.clone()),
            // Depth is the delegation chain length; Shapiro reports it stays at 3 or less.
            Target::Via(f) => f.target.lock().as_ref().ok_or(CapError::Revoked)?.resolve(),
        }
    }
}

#[derive(Clone)]
pub struct Cap {
    target: Target,
    rights: Rights,
}

impl Cap {
    pub fn new(obj: Object, rights: Rights) -> Self {
        Self {
            target: Target::Direct(obj),
            rights,
        }
    }

    pub fn rights(&self) -> Rights {
        self.rights
    }

    /// The object, if this capability carries `need` and has not been revoked.
    pub fn resolve(&self, need: Rights) -> Result<Object, CapError> {
        if !self.rights.contains(need) {
            return Err(CapError::Rights);
        }
        self.target.resolve()
    }

    /// An attenuated copy: rights can only shrink. Shares this capability's revocation fate.
    pub fn derive(&self, mask: Rights) -> Result<Cap, CapError> {
        self.resolve(Rights::GRANT)?;
        Ok(Cap {
            target: self.target.clone(),
            rights: self.rights.mask(mask),
        })
    }

    /// An attenuated copy behind a fresh forwarder. The caller (the broker) keeps the forwarder
    /// and revokes this one grant with it.
    pub fn grant_revocable(&self, mask: Rights) -> Result<(Cap, Arc<Forwarder>), CapError> {
        self.resolve(Rights::GRANT)?;
        let fwd = Arc::new(Forwarder {
            target: SpinLock::new(Some(self.target.clone())),
        });
        let cap = Cap {
            target: Target::Via(fwd.clone()),
            rights: self.rights.mask(mask),
        };
        Ok((cap, fwd))
    }
}

pub struct CSpace {
    slots: Vec<Option<Cap>>,
    /// Indices of `None` slots in `slots`.
    free: Vec<usize>,
}

impl Default for CSpace {
    fn default() -> Self {
        Self::new()
    }
}

impl CSpace {
    /// Bound on slots per agent; keeps an agent from growing kernel memory without limit.
    pub const MAX_SLOTS: usize = 65_536;

    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    /// Lowest-free reuse is not guaranteed. POSIX lowest-fd semantics belong to the fd table in
    /// the vfs/procmgr phases, not to the CSpace.
    pub fn insert(&mut self, cap: Cap) -> Result<usize, CapError> {
        if let Some(i) = self.free.pop() {
            self.slots[i] = Some(cap);
            return Ok(i);
        }
        if self.slots.len() >= Self::MAX_SLOTS {
            return Err(CapError::Full);
        }
        self.slots.push(Some(cap));
        Ok(self.slots.len() - 1)
    }

    pub fn get(&self, slot: usize) -> Result<&Cap, CapError> {
        self.slots
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or(CapError::NoSlot)
    }

    pub fn remove(&mut self, slot: usize) -> Result<Cap, CapError> {
        let cap = self
            .slots
            .get_mut(slot)
            .and_then(Option::take)
            .ok_or(CapError::NoSlot)?;
        self.free.push(slot);
        Ok(cap)
    }
}

#[cfg(ktest)]
mod test {
    use ostd::prelude::*;

    use super::*;

    fn ep_cap(rights: Rights) -> Cap {
        Cap::new(Object::Endpoint(Endpoint::new()), rights)
    }

    #[ktest]
    fn derive_never_amplifies() {
        let c = ep_cap(Rights::SEND.union(Rights::GRANT));
        let d = c.derive(Rights::ALL).unwrap();
        assert_eq!(d.rights(), Rights::SEND.union(Rights::GRANT));
        assert_eq!(d.resolve(Rights::RECV).err(), Some(CapError::Rights));
    }

    #[ktest]
    fn derive_requires_grant() {
        let c = ep_cap(Rights::SEND);
        assert_eq!(c.derive(Rights::SEND).err(), Some(CapError::Rights));
        assert_eq!(
            c.grant_revocable(Rights::SEND).err(),
            Some(CapError::Rights)
        );
    }

    #[ktest]
    fn revoke_kills_one_grant_and_its_descendants() {
        let root = ep_cap(Rights::ALL);
        let (a, fwd_a) = root.grant_revocable(Rights::ALL).unwrap();
        let (b, _fwd_b) = root.grant_revocable(Rights::SEND).unwrap();
        let a_child = a.derive(Rights::SEND).unwrap();
        let (a_regrant, _) = a.grant_revocable(Rights::SEND).unwrap();

        fwd_a.revoke();

        assert_eq!(a.resolve(Rights::SEND).err(), Some(CapError::Revoked));
        assert_eq!(a_child.resolve(Rights::SEND).err(), Some(CapError::Revoked));
        assert_eq!(
            a_regrant.resolve(Rights::SEND).err(),
            Some(CapError::Revoked)
        );
        assert!(b.resolve(Rights::SEND).is_ok());
        assert!(root.resolve(Rights::SEND).is_ok());
    }

    #[ktest]
    fn cspace_slots() {
        let mut cs = CSpace::new();
        let s = cs.insert(ep_cap(Rights::SEND)).unwrap();
        assert_eq!(s, 0);
        assert!(cs.get(s).is_ok());
        cs.remove(s).unwrap();
        assert_eq!(cs.get(s).err(), Some(CapError::NoSlot));
        assert_eq!(cs.remove(s).err(), Some(CapError::NoSlot));
        assert_eq!(cs.get(CSpace::MAX_SLOTS).err(), Some(CapError::NoSlot));
        let cap = ep_cap(Rights::SEND);
        for i in 0..CSpace::MAX_SLOTS {
            assert_eq!(cs.insert(cap.clone()).unwrap(), i);
        }
        assert_eq!(cs.insert(cap.clone()).err(), Some(CapError::Full));
        // Freeing one slot makes room again without growing past the bound.
        cs.remove(7).unwrap();
        assert_eq!(cs.insert(cap).unwrap(), 7);
    }

    #[ktest]
    fn cspace_grows_past_64_with_install_and_derive() {
        let mut cs = CSpace::new();
        for _ in 0..200 {
            cs.insert(ep_cap(Rights::ALL)).unwrap();
        }
        for slot in 64..200 {
            let d = cs.get(slot).unwrap().derive(Rights::SEND).unwrap();
            let ds = cs.insert(d).unwrap();
            assert!(ds >= 200);
            let derived = cs.get(ds).unwrap();
            assert!(derived.resolve(Rights::SEND).is_ok());
            assert_eq!(derived.resolve(Rights::RECV).err(), Some(CapError::Rights));
            assert!(cs.get(slot).unwrap().resolve(Rights::SEND).is_ok());
        }
    }

    #[ktest]
    fn cspace_reuses_freed_slots() {
        let mut cs = CSpace::new();
        for _ in 0..100 {
            cs.insert(ep_cap(Rights::SEND)).unwrap();
        }
        for slot in 70..80 {
            cs.remove(slot).unwrap();
        }
        let mut got: Vec<usize> = (0..10)
            .map(|_| cs.insert(ep_cap(Rights::SEND)).unwrap())
            .collect();
        got.sort_unstable();
        assert_eq!(got, (70..80).collect::<Vec<_>>());
        assert_eq!(cs.slots.len(), 100);
    }

    #[ktest]
    fn revoke_reaches_copies_above_slot_64() {
        let root = ep_cap(Rights::ALL);
        let (granted, fwd) = root.grant_revocable(Rights::ALL).unwrap();
        let mut cs = CSpace::new();
        cs.insert(ep_cap(Rights::SEND)).unwrap();
        for _ in 0..100 {
            cs.insert(granted.derive(Rights::SEND).unwrap()).unwrap();
        }
        fwd.revoke();
        for slot in 1..=100 {
            assert_eq!(
                cs.get(slot).unwrap().resolve(Rights::SEND).err(),
                Some(CapError::Revoked)
            );
        }
        assert!(cs.get(0).unwrap().resolve(Rights::SEND).is_ok());
    }

    #[ktest]
    fn spawn_child_grants_from_slot_above_64() {
        let parent = Agent::new_root(1000, 10);
        {
            let mut cs = parent.cspace.lock();
            let ep = Object::Endpoint(Endpoint::new());
            for _ in 0..100 {
                cs.insert(Cap::new(ep.clone(), Rights::ALL)).unwrap();
            }
        }
        let child = parent.spawn_child(&[(99, Rights::SEND)], 1, 1).unwrap();
        let cs = child.cspace.lock();
        let c = cs.get(0).unwrap();
        assert!(c.resolve(Rights::SEND).is_ok());
        assert_eq!(c.resolve(Rights::GRANT).err(), Some(CapError::Rights));
    }
}
