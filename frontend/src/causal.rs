// Causality barrier
// Keeps for each known peer, keeps track of the latest clock seen
// And a set of messages that are from the future
// and outputs the full-in-order sequence of messages
//
//

use derive_more::{Add, From, Into};
use serde::{Deserialize, Serialize};
use std::collections::*;

#[derive(PartialEq, Eq, Debug, Copy, Clone, Hash, PartialOrd, Ord, From, Deserialize, Serialize)]
pub struct SiteId(pub u32);

#[derive(PartialEq, Eq, Debug, Copy, Clone, PartialOrd, Ord, Hash, Add, From, Into, Deserialize, Serialize)]
pub struct LogTime(u64);

/// Replace with the real identifier
pub struct TempId(u32);
/// Version Vector with Exceptions
pub struct CausalityBarrier<T: CausalOp> {
    peers: HashMap<SiteId, VectorEntry>,
    // Do we really need a map or just a set?
    local_id: SiteId,
    pub buffer: HashMap<(SiteId, LogTime), T>,
}

#[derive(Serialize, Deserialize)]
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

    /// Calculate the difference between a remote VectorEntry and ours.
    /// Specifically, we want the set of operations we've seen that the remote hasn't
    pub fn diff_from(&self, other: &Self) -> HashSet<LogTime> {
        // 1. Find (new) operations that we've seen locally that the remote hasn't
        let local_ops = (other.max_version.into()..self.max_version.into()).into_iter().filter(|ix : &u64| {
            !self.exceptions.contains(&(*ix).into())
        }).map(LogTime::from);

        // 2. Find exceptions that we've seen.
        let mut local_exceptions  = other.exceptions.difference(&self.exceptions).map(|ix| ix.to_owned());

        local_ops.chain(&mut local_exceptions).collect()
    }
}

pub trait CausalOp {
    /// Tells us the id we causally depend on
    /// Remove(X) depends on X
    /// Insert(Y) depends on nothing.
    fn happens_before(&self) -> bool;
    fn site(&self) -> SiteId;
    fn clock(&self) -> LogTime;
}

impl<T: CausalOp> CausalityBarrier<T> {
    pub fn new(site_id: SiteId) -> Self {
        CausalityBarrier { peers: HashMap::new(), buffer: HashMap::new(), local_id: site_id }
    }

    pub fn ingest(&mut self, msg: CausalMessage<T>) -> Option<T> {
        let v = self.peers.entry(msg.local_id).or_insert_with(VectorEntry::new);

        v.increment(msg.time);

        // Ok so it's an exception but maybe we can still integrate it if it's not constrained
        // by a happens-before relation.
        // For example: we can always insert into most CRDTs but we can only delete if the
        // corresponding insert happened before!
        match msg.msg.happens_before() {
            // Dang! we have a happens before relation!
            true => {
                // Let's buffer this operation then.
                self.buffer.insert((msg.msg.site(), msg.msg.clock()), msg.msg);
                // and do nothing
                None
            }
            false => {
                // Ok so we're not causally constrained, but maybe we already saw an associated
                // causal operation? If so let's just delete the pair
                match self.buffer.remove(&(msg.msg.site(), msg.msg.clock())) {
                    Some(_) => None,
                    None => Some(msg.msg),
                }
            }
        }
    }

    pub fn expel(&mut self, msg: T) -> T {
        self.peers.entry(self.local_id).or_insert_with(VectorEntry::new)
            .max_version = msg.clock();

        msg
    }

    pub fn diff_from(&self, other: &HashMap<SiteId, VectorEntry>) -> HashMap<SiteId, HashSet<LogTime>> {
        let mut ret = HashMap::new();
        for (site_id, entry) in self.peers.iter() {
            let e_diff = match other.get(site_id) {
                Some(remote_entry) => entry.diff_from(remote_entry),
                None => (0..entry.max_version.into()).map(LogTime::from).collect(),
            };
            ret.insert(*site_id, e_diff);
        }
        ret 
    }

}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(PartialEq, Debug)]
    enum Op {
        Insert(u64),
        Delete(u64),
    }

    use Op::*;

    impl CausalOp for Op {
        fn happens_before(&self) -> bool {
            match self {
                Op::Insert(_) => false,
                Op::Delete(_i) => true,
            }
        }

        fn clock(&self) -> LogTime {
            match self {
                Insert(i) => LogTime::from(*i),
                Delete(i) => LogTime::from(*i),
            }
        }
        fn site(&self) -> SiteId {
            SiteId(0)
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

    #[test]
    fn entry_diff_new_entries() {
        let a = VectorEntry::new();
        let b = VectorEntry { max_version: 10.into(), exceptions: HashSet::new() };

        let c : HashSet<LogTime> = (0..10).into_iter().map(LogTime::from).collect();
        assert_eq!(b.diff_from(&a), c);
    }


    #[test]
    fn entry_diff_found_exceptions() {
        let a = VectorEntry { max_version: 10.into(), exceptions: [1,2,3,4].iter().map(|i|LogTime::from(*i)).collect() };
        let b = VectorEntry { max_version: 5.into(), exceptions: HashSet::new() };

        let c : HashSet<LogTime> = [1,2,3,4].iter().map(|i|LogTime::from(*i)).collect();
        assert_eq!(b.diff_from(&a), c);
    }

    #[test]
    fn entry_diff_complex() {
        // a has seen 0, 5
        let a = VectorEntry { max_version: 6.into(), exceptions: [1,2,3,4].iter().map(|i|LogTime::from(*i)).collect() };
        // b has seen 0, 1, 5,6,7,8
        let b = VectorEntry { max_version: 9.into(), exceptions:  [2,3, 4].iter().map(|i|LogTime::from(*i)).collect() };

        // c should be 1,6,7,8
        let c : HashSet<LogTime> = [1,6,7,8].iter().map(|i|LogTime::from(*i)).collect();
        assert_eq!(b.diff_from(&a), c);
    }
}
