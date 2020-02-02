// Causality barrier
// Keeps for each known peer, keeps track of the latest clock seen
// And a set of messages that are from the future
// and outputs the full-in-order sequence of messages
//
//

use derive_more::{Add, From};
use std::collections::*;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Copy, Clone, Hash, PartialOrd, Ord, From, Deserialize, Serialize)]
pub struct SiteId(pub u32);

#[derive(PartialEq, Eq, Debug, Copy, Clone, PartialOrd, Ord, Hash, Add, From, Deserialize, Serialize)]
pub struct LogTime(u32);

/// Replace with the real identifier
pub struct TempId(u32);
/// Version Vector with Exceptions
pub struct CausalityBarrier<T: CausalOp> {
    local_clock: LogTime,
    local_id: SiteId,
    peers: HashMap<SiteId, VectorEntry>,
    // Do we really need a map or just a set?
    buffer: HashMap<T::Id, T>,
}

pub struct VectorEntry {
    max_version: LogTime,
    exceptions: HashSet<LogTime>,
}

#[derive(PartialEq, Debug, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct CausalMessage<T> {
    time: LogTime,
    local_id: SiteId,
    msg: T,
}

use std::hash::Hash;

#[derive(From, PartialEq, Eq)]
pub struct Exception(bool);

impl VectorEntry {
    pub fn new() -> VectorEntry {
        VectorEntry { max_version: 0.into(), exceptions: HashSet::new() }
    }

    pub fn increment(&mut self, clk: LogTime) {
        // We've just found an exception
        if clk < self.max_version {
            self.exceptions.take(&clk);
        } else if clk == self.max_version + 1.into() {
            self.max_version = self.max_version + 1.into();
        } else {
            let mut x = self.max_version + 1.into();
            while x < clk {
                self.exceptions.insert(x);
                x = x + 1.into();
            }
        }
    }

    pub fn is_ready(&self, clk: LogTime) -> bool {
        clk <= self.max_version && !self.exceptions.contains(&clk)
    }
}

pub trait CausalOp {
    type Id: Eq + Hash;

    /// Tells us the id we causally depend on
    /// Remove(X) depends on X
    /// Insert(Y) depends on nothing.
    fn happens_before(&self) -> bool;
    fn id(&self) -> Self::Id;
}

impl<T: CausalOp> CausalityBarrier<T> {
    pub fn new(site_id: SiteId) -> Self {
        CausalityBarrier { local_id: site_id, local_clock: 0.into(), peers: HashMap::new(), buffer: HashMap::new() }
    }
    pub fn ingest(&mut self, msg: CausalMessage<T>) -> Option<T> {
        let v = self.peers.entry(msg.local_id).or_insert_with(|| VectorEntry::new());

        v.increment(msg.time);

        // Ok so it's an exception but maybe we can still integrate it if it's not constrained
        // by a happens-before relation.
        // For example: we can always insert into most CRDTs but we can only delete if the
        // corresponding insert happened before!
        match msg.msg.happens_before() {
            // Dang! we have a happens before relation!
            true => {
                // Let's buffer this operation then.
                self.buffer.insert(msg.msg.id(), msg.msg);
                // and do nothing
                None
            }
            false => {
                // Ok so we're not causally constrained, but maybe we already saw an associated
                // causal operation? If so let's just delete the pair
                match self.buffer.remove(&msg.msg.id()) {
                    Some(_) => None,
                    None => Some(msg.msg),
                }
            }
        }
    }

    pub fn expel(&mut self, msg: T) -> CausalMessage<T> {
        let t = self.local_clock;
        self.local_clock = LogTime(t.0 + 1);

        CausalMessage { time: t, local_id: self.local_id, msg: msg }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(PartialEq, Debug)]
    enum Op {
        Insert(u32),
        Delete(u32),
    }

    use Op::*;

    impl CausalOp for Op {
        type Id = u32;

        fn happens_before(&self) -> bool {
            match self {
                Op::Insert(_) => false,
                Op::Delete(_i) => true,
            }
        }

        fn id(&self) -> Self::Id {
            match self {
                Insert(i) => *i,
                Delete(i) => *i,
            }
        }
    }

    #[test]
    fn delete_before_insert() {
        let mut barrier = CausalityBarrier::new(0.into());

        let del = CausalMessage { time: 0.into(), local_id: 1.into(), msg: Op::Delete(0) };
        let ins = CausalMessage { time: 1.into(), local_id: 1.into(), msg: Op::Insert(0) };
        assert_eq!(barrier.ingest(del), None);
        assert_eq!(barrier.ingest(ins), None);
    }

    #[test]
    fn insert() {
        let mut barrier = CausalityBarrier::new(0.into());

        let ins = CausalMessage { time: 1.into(), local_id: 1.into(), msg: Op::Insert(0) };
        assert_eq!(barrier.ingest(ins), Some(Op::Insert(0)));
    }

    #[test]
    fn delete_before_insert_multiple_sites() {
        let mut barrier = CausalityBarrier::new(0.into());

        let del = CausalMessage { time: 0.into(), local_id: 2.into(), msg: Op::Delete(0) };
        let ins = CausalMessage { time: 5.into(), local_id: 1.into(), msg: Op::Insert(0) };
        assert_eq!(barrier.ingest(del), None);
        assert_eq!(barrier.ingest(ins), None);
    }
}
