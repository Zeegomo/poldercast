use crate::{Id, Node, Policy, PolicyReport};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Nodes {
    all: LruCache<Id, Node>,
    quarantined: HashSet<Id>,
    not_reachable: HashSet<Id>,
    available: HashSet<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Count {
    pub all_count: usize,
    pub quarantined_count: usize,
    pub not_reachable_count: usize,
    pub available_count: usize,
}

pub enum Entry<'a> {
    Vacant(VacantEntry<'a>),
    Occupied(OccupiedEntry<'a>),
}

pub struct VacantEntry<'a> {
    nodes: &'a mut Nodes,
    id: Id,
}

pub struct OccupiedEntry<'a> {
    id: Id,
    nodes: &'a mut Nodes,
}

impl Nodes {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            all: LruCache::new(cap),
            quarantined: HashSet::new(),
            not_reachable: HashSet::new(),
            available: HashSet::new(),
        }
    }

    pub fn peek<'a>(&'a self, id: &Id) -> Option<&'a Node> {
        self.all.peek(id)
    }

    pub fn peek_mut<'a>(&'a mut self, id: &Id) -> Option<&'a mut Node> {
        self.all.peek_mut(id)
    }

    pub(crate) fn get<'a>(&'a mut self, id: &Id) -> Option<&'a Node> {
        self.all.get(id)
    }

    pub(crate) fn get_mut<'a>(&'a mut self, id: &Id) -> Option<&'a mut Node> {
        self.all.get_mut(id)
    }

    pub fn entry(&mut self, public_id: Id) -> Entry<'_> {
        if self.all.contains(&public_id) {
            Entry::Occupied(OccupiedEntry::new(self, public_id))
        } else {
            Entry::Vacant(VacantEntry::new(self, public_id))
        }
    }

    pub fn available_nodes(&self) -> &HashSet<Id> {
        &self.available
    }

    /// list all available nodes, these are nodes that are not quarantined
    /// and that are publicly reachable.
    ///
    /// This operation is costly and should not be used often or it will slow
    /// down the other operation of the `Nodes`
    pub fn all_available_nodes(&self) -> Vec<&Node> {
        self.available_nodes()
            .iter()
            .filter_map(|id| self.all.peek(id))
            .collect()
    }

    /// list all quarantined nodes, these are nodes that are not in used in the
    /// p2p topology and but may become available or be removed soon.
    ///
    /// This operation is costly and should not be used often or it will slow
    /// down the other operation of the `Nodes`
    pub fn all_quarantined_nodes(&self) -> Vec<&Node> {
        self.quarantined_nodes()
            .iter()
            .filter_map(|id| self.all.peek(id))
            .collect()
    }

    /// list all non publicly reachable nodes. These are nodes that are directly
    /// connected to our nodes and that are not gossiped about.
    ///
    /// This operation is costly and should not be used often or it will slow
    /// down the other operation of the `Nodes`
    pub fn all_unreachable_nodes(&self) -> Vec<&Node> {
        self.unreachable_nodes()
            .iter()
            .filter_map(|id| self.all.peek(id))
            .collect()
    }

    /// access nodes that are connected to us but not necessarily reachable
    ///
    /// This can be nodes that are behind a firewall or a NAT and that can't do
    /// hole punching to allow other nodes to connect to them.
    pub fn unreachable_nodes(&self) -> &HashSet<Id> {
        &self.not_reachable
    }

    pub fn quarantined_nodes(&self) -> &HashSet<Id> {
        &self.quarantined
    }

    /// access a count of all nodes
    pub fn node_count(&self) -> Count {
        Count {
            all_count: self.all.len(),
            available_count: self.available.len(),
            not_reachable_count: self.not_reachable.len(),
            quarantined_count: self.quarantined.len(),
        }
    }

    fn insert(&mut self, node: Node) -> Option<Node> {
        let id = *node.id();
        if node.address().is_some() {
            self.available.insert(id);
        } else {
            self.not_reachable.insert(id);
        };

        // prevent entering an element that will trigger removing
        // the least recently used entry (this is something missing
        // from the `lru` crate that would be improved: detecting when
        // a `put` removed an entry, though it can be simulated here
        // with the following checks):
        while self.all.len() >= self.all.cap() {
            if let Some((k, _)) = self.all.pop_lru() {
                self.available.remove(&k);
                self.quarantined.remove(&k);
                self.not_reachable.remove(&k);
            }
        }
        self.all.put(id, node)
    }

    pub(crate) fn reset<P>(&mut self, policy: &mut P)
    where
        P: Policy,
    {
        let available = &mut self.available;
        let not_reachable = &mut self.not_reachable;
        let quarantined = &mut self.quarantined;

        let mut to_remove = Vec::new();

        for (k, node) in self.all.iter_mut() {
            let report = policy.check(node);

            match report {
                PolicyReport::None => (),
                PolicyReport::Forget => {
                    available.remove(k);
                    not_reachable.remove(k);
                    quarantined.remove(k);

                    to_remove.push(k.clone());
                }
                PolicyReport::Quarantine => {
                    available.remove(k);
                    not_reachable.remove(k);
                    quarantined.insert(k.clone());
                    node.logs_mut().quarantine();
                }
                PolicyReport::LiftQuarantine => {
                    if node.address().is_some() {
                        available.insert(k.clone());
                    } else {
                        not_reachable.insert(k.clone());
                    }
                    quarantined.remove(k);
                    node.logs_mut().lift_quarantine();
                }
            }
        }

        for k in to_remove {
            self.all.pop(&k);
        }
    }
}

impl<'a> VacantEntry<'a> {
    fn new(nodes: &'a mut Nodes, id: Id) -> Self {
        VacantEntry { nodes, id }
    }

    pub(crate) fn insert(&mut self, default: Node) {
        debug_assert_eq!(self.key(), default.id());
        assert!(self.nodes.insert(default).is_none());
    }

    fn key(&self) -> &Id {
        &self.id
    }
}

impl<'a> OccupiedEntry<'a> {
    fn new(nodes: &'a mut Nodes, id: Id) -> Self {
        OccupiedEntry { nodes, id }
    }

    fn key(&self) -> &Id {
        &self.id
    }

    pub(crate) fn modify<P, F>(&mut self, policy: &mut P, f: F) -> PolicyReport
    where
        F: FnOnce(&mut Node),
        P: Policy,
    {
        let node = self.nodes.all.get_mut(&self.id).unwrap();
        let was_reachable = node.address().is_some();
        f(node);
        let report = policy.check(node);

        match report {
            PolicyReport::None => {
                let now_reachable = node.address().is_some();
                if was_reachable && !now_reachable {
                    if self.nodes.available.remove(&self.id) {
                        self.nodes.not_reachable.insert(self.id);
                    }
                } else if !was_reachable
                    && now_reachable
                    && self.nodes.not_reachable.remove(&self.id)
                {
                    self.nodes.available.insert(self.id);
                }
            }
            PolicyReport::Forget => {
                self.nodes.available.remove(&self.id);
                self.nodes.not_reachable.remove(&self.id);
                self.nodes.quarantined.remove(&self.id);
                self.nodes.all.pop(&self.id);
            }
            PolicyReport::Quarantine => {
                self.nodes.available.remove(&self.id);
                self.nodes.not_reachable.remove(&self.id);
                self.nodes.quarantined.insert(self.id);
                node.logs_mut().quarantine();
            }
            PolicyReport::LiftQuarantine => {
                if node.address().is_some() {
                    self.nodes.available.insert(self.id);
                } else {
                    self.nodes.not_reachable.insert(self.id);
                }
                self.nodes.quarantined.remove(&self.id);
                node.logs_mut().lift_quarantine();
            }
        }

        report
    }
}

impl<'a> Entry<'a> {
    /// Ensures a value is in the entry by inserting the default if empty,
    /// and returns a mutable reference to the value in the entry.
    pub fn or_insert(self, default: Node) {
        match self {
            Entry::Vacant(mut node_entry) => node_entry.insert(default),
            Entry::Occupied(_node_entry) => {}
        }
    }

    /// Ensures a value is in the entry by inserting the result of the default function
    /// if empty, and returns a mutable reference to the value in the entry
    ///
    /// The advantage of this function over `or_insert` is that it is called only if
    /// the field was vacant
    pub fn or_insert_with<F>(self, default: F)
    where
        F: FnOnce() -> Node,
    {
        match self {
            Entry::Vacant(mut node_entry) => node_entry.insert(default()),
            Entry::Occupied(_node_entry) => {}
        }
    }

    pub fn key(&self) -> &Id {
        match self {
            Entry::Vacant(node_entry) => node_entry.key(),
            Entry::Occupied(node_entry) => node_entry.key(),
        }
    }

    /// Provides in-place mutable access to an occupied entry before any potential
    /// inserts into the collection
    pub fn and_modify<P, F>(self, policy: &mut P, f: F) -> Option<PolicyReport>
    where
        F: FnOnce(&mut Node),
        P: Policy,
    {
        match self {
            Entry::Occupied(mut node_entry) => Some(node_entry.modify(policy, f)),
            Entry::Vacant(_) => None,
        }
    }
}
