use rand::Rng;

use rand::distributions::Alphanumeric;

const BOUNDARY: usize = 10;

const INITIAL_BASE: u32 = 3; // start with 2^8

#[derive(Debug, PartialEq, PartialOrd, Ord, Eq, Clone)]
pub struct Identifier {
    path: Vec<usize>,
    site_id: Vec<u64>,
    counter: u64
}

pub struct IdentGen {
    initial_base_bits: u32,
    site_id: u64,
    clock: u64
}

impl IdentGen {
    pub fn new() -> IdentGen {
        Self::new_with_args(INITIAL_BASE, 0)
    }

    pub fn new_with_args(base: u32, site_id: u64) -> IdentGen {
        IdentGen { initial_base_bits: INITIAL_BASE, site_id: site_id, clock: 0 }
    }

    pub fn lower(&self) -> Identifier {
        Identifier { path: vec![0], site_id: vec![self.site_id], counter: 0 }
    }

    pub fn upper(&self) -> Identifier {
        Identifier { path: vec![2usize.pow(self.initial_base_bits) - 1], site_id: vec![self.site_id], counter: 0 }
    }
    /// Allocates a new identifier between p and q.
    /// Requires that p < q and will produce a new identifier z, p < z < q
    pub fn alloc(&mut self, p: &Identifier, q: &Identifier) -> Identifier {
        assert!(p < q, "lower bound should be smaller than upper bound!");
        let mut depth = 0;

        // Descend both identifiers in parallel until we find room to insert an identifier
        loop {
            match (p.path.get(depth), q.path.get(depth)) {
                // Our descent continues...
                (Some(a), Some(b)) if a == b => { }
                // We found a gap between the two identifiers
                (Some(a), Some(b)) if *a + 1 < *b => {
                    // is there room at this level?
                    if *a + 1 < *b {
                        // great! let's allocate an identifier at this depth then
                        let next_index = self.index_in_range(*a + 1, *b, depth as u32);
                        return self.replace_last(p, depth, next_index);
                    } else {
                        // Search for room at a lower level (so depth + 1)
                        return self.alloc_with_lower(p, depth + 1);
                    }
                }

                // The upper bound ended on the previous level.
                // This means we can allocate a node in the range a..max_for_depth
                // If there isn't room at this level because _a = max_for_depth then we'll keep
                // searching below.
                (Some(_a), _) => { return self.alloc_with_lower(p, depth); }
                // Because the upper bound is zero, we're forced to create a path with zeros until
                // the upper bound is either empty or non-zero
                (None, Some(0)) => { return self.alloc_with_upper(p, q, depth); }
                (None, Some(b)) => {
                    let next_index = self.index_in_range(0, *b, depth as u32);
                    return self.push_index(p, next_index);
                }
                // The two paths are fully equal which means that the site_ids MUST be different or
                // we are in an invalid situation
                (None, None) => {
                    let max_for_depth = 2usize.pow(self.initial_base_bits + depth as u32) - 1;
                    let next_index = self.index_in_range(0, max_for_depth, depth as u32);
                    return self.push_index(p, next_index);
                }
            };
            depth += 1;
        }
    }

    // Here the problem is that we've run out of the lower path and the upper one is zero!
    // The idea is to keep pushing 0s onto the lower path until we can find a new level to allocate at.
    fn alloc_with_upper(&mut self, p: &Identifier, q: &Identifier, mut depth: usize) -> Identifier {
        assert!(p.path.len() <= depth);
        assert!(q.path.len() > depth);
        assert!(q.path[depth] == 0);

        let mut ident = p.clone();
        loop {
            match q.path.get(depth) {
                // append a 0 to the result path as well
                Some(0) => ident.path.push(0),
                // oo! a non-zero value
                _ => break,
            }
            depth += 1;
        };

        // If we actually ran out of upper bound values then we're free to choose
        // anything on the next level, otherwise use the upper bound we've found.
        let upper = match q.path.get(depth + 1) {
            Some(b) => *b,
            None => self.width_at(depth + 1),
        };
        let next_index = self.index_in_range(0, upper, depth as u32);
        return self.push_index(&ident, next_index);
    }

    //
    fn alloc_with_lower(&mut self, p: &Identifier, depth: usize) -> Identifier {
        let mut lower_bound = depth;

        loop {
            match p.path.get(lower_bound) {
                Some(ix) if ix + 1 < self.width_at(lower_bound) => {
                    let next_index = self.index_in_range(*ix, self.width_at(lower_bound), lower_bound as u32);
                    return self.push_index(p, next_index);
                }
                Some(_) => { }
                None => {
                    let next_index = self.index_in_range(0, self.width_at(lower_bound), lower_bound as u32);
                    return self.push_index(p, next_index);
                }
            }
            lower_bound += 1;
        }

    }

    fn replace_last(&mut self, p: &Identifier, depth: usize, ix: usize) -> Identifier {
        let mut ident = p.clone();
        ident.path.truncate(depth);
        ident.path.push(ix);
        ident.site_id.truncate(depth);
        ident.site_id.push(self.site_id);
        ident.counter = self.clock;
        self.clock += 1;
        return ident;
    }

    fn push_index(&mut self, p: &Identifier, ix: usize) -> Identifier {
        let mut ident = p.clone();
        ident.path.push(ix);
        ident.site_id.push(self.site_id);
        ident.counter = self.clock;
        self.clock += 1;
        return ident;
    }

    fn width_at(&self, depth: usize) -> usize {
        2usize.pow(self.initial_base_bits + depth as u32)
    }
    // Generate an index in a given range at the specified depth.
    // Uses the allocation strategy of that depth, boundary+ or boundary- which is biased to the
    // lower and upper ends of the range respectively.
    // should allocate in the range [lower, upper)
    fn index_in_range(&mut self, lower: usize, upper: usize, depth: u32) -> usize {
        assert!(lower < upper, "need at least one space between the bounds lower={} upper={}", lower, upper);

        let mut rng = rand::thread_rng();
        let interval = BOUNDARY.min(upper - 1 - lower);
        let step = if interval > 0 { rng.gen_range(0, interval) } else { 0 };
        if self.strategy(depth) {
            //boundary+

            lower + step
        } else {
            //boundary-

            upper - step - 1
        }
    }

    fn strategy(&mut self, depth: u32) -> bool {
        // temp strategy. Should be a random choice at each level
        depth % 2 == 0
    }
}

pub struct LSeq {
    text: Vec<(Identifier, u64)>, // gen: NameGenerator
    gen: IdentGen,
}

#[derive(Debug)]
pub enum Op {
    Insert(Identifier, u64),
    Delete(Identifier),
}

impl LSeq {
    pub fn do_insert(&mut self, ix: Identifier, c: u64) {
        let res = self.text.binary_search_by(|e| e.0.cmp(&ix));

        match res {
            Ok(_) => {
                panic!("tried to insert an index that's already in!! {:?} {:?}", ix, self.gen.site_id);
            }
            // The index doesn't exist in our current text
            Err(i) => {
                // the index we want to insert is outside the current range
                self.text.insert(i, (ix, c));
            }
        }
    }

    pub fn do_delete(&mut self, ix: Identifier) {
        if let Ok(i) = self.text.binary_search_by(|e| e.0.cmp(&ix)) {
            self.text.remove(i);
        }
    }

    pub fn apply(&mut self, op: Op) {
        match op {
            Op::Insert(id, c) => self.do_insert(id, c),
            Op::Delete(id) => self.do_delete(id),
        }
    }

    // Perform a local insertion and create the operation that should be broadcast
    pub fn local_insert(&mut self, ix: usize, c: char) -> Op {
        let lower = self.gen.lower();
        let upper = self.gen.upper();
        // append!
        let ix_ident = if self.text.len() <= ix {
            let prev = self.text.last().map(|(i, _)| i).unwrap_or_else(|| &lower);
            println!("append!");
            self.gen.alloc(prev, &upper)
        } else {
            let prev = self.text.get(ix).map(|(i, _)| i).unwrap();
            let next = self.text.get(ix + 1).map(|(i, _)| i).unwrap_or(&upper);
            let a = self.gen.alloc(prev, next);
            println!("ix={:?} len={:?} lower={:?} a={:?} upper={:?}", ix, self.text.len(), prev, a, next);

            assert!(prev < &a); assert!(&a < next);
            a
        };
        self.do_insert(ix_ident.clone(), c as u64);

        Op::Insert(ix_ident, c as u64)
    }

    pub fn local_delete(&mut self, ix: usize) -> Op {
        let ident = self.text[ix].0.clone();

        self.do_delete(ident.clone());

        Op::Delete(ident.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use quickcheck::{Arbitrary, Gen, TestResult};

    impl Arbitrary for Identifier {
        fn arbitrary<G: Gen>(g: &mut G) -> Identifier {
            Identifier { path: Vec::<usize>::arbitrary(g), site_id: Vec::<u64>::arbitrary(g), counter: u64::arbitrary(g) }
        }
    }

    #[quickcheck]
    fn prop_alloc(p: Identifier, q: Identifier) -> TestResult {
        if (p >= q || p.path.len() == 0 || q.path.len() == 0) { return TestResult::discard(); }
        let mut gen = IdentGen::new();
        let z = gen.alloc(&p, &q);

        TestResult::from_bool(p < z && z < q)

    }
    #[test]
    fn test_alloc_eq_path() {
        let mut gen = IdentGen::new();

        let x = Identifier { path: vec![1, 0], site_id: vec![0], counter: 0};
        let y = Identifier { path: vec![1, 0], site_id: vec![1], counter: 0};
        gen.alloc(&x, &y);
        let b = gen.alloc(&x, &y);
        println!("{:?} {:?} {:?}", x, b, y);
        assert!(x < b);
        assert!(b < y);
    }

    #[test]
    fn test_different_len_paths() {
        let mut gen = IdentGen::new();
        let x = Identifier { path: vec![1], site_id: vec![0], counter: 0};
        let y = Identifier { path: vec![1, 15], site_id: vec![0], counter: 0};

        let z = gen.alloc(&x, &y);

        assert!(x < z);
        assert!(z < y);
    }

    #[test]
    fn test_alloc() {
        let mut gen = IdentGen::new();
        let a = Identifier { path: vec![1], site_id: vec![0], counter: 0};
        let b = Identifier { path: vec![3], site_id: vec![0], counter: 0};

        assert_eq!(gen.alloc(&a, &b), Identifier { path: vec![2], site_id: vec![0], counter: 0 });

        let c = Identifier { path: vec![1, 0, 1], site_id: vec![0, 0, 0], counter: 0};
        let d = Identifier { path: vec![1, 0, 3], site_id: vec![0, 0, 0], counter: 0};

        assert_eq!(gen.alloc(&c, &d), Identifier { path: vec![1, 0, 2], site_id: vec![0, 0, 0], counter: 1 });

        let e = Identifier { path: vec![1], site_id: vec![0], counter: 0};
        let f = Identifier { path: vec![2], site_id: vec![0], counter: 0};

        let res = gen.alloc(&e, &f);

        assert!(e < res);
        assert!(res < f);

    }

    #[test]
    fn test_index_in_range() {
        let mut gen = IdentGen::new();
        assert_eq!(gen.index_in_range(0, 1, 1), 0);
    }

    #[test]
    fn test_inserts () {
        // A simple smoke test to ensure that insertions work properly.
        // Uses two sites which random insert a character and then immediately insert it into the
        // other site.
        let mut rng = rand::thread_rng();

        let mut s1 = rng.sample_iter(Alphanumeric);
        let mut s2 = rng.sample_iter(Alphanumeric);
        let mut site1 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, 0) };
        let mut site2 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, 1) };

        for _ in 0..50 {
            if rng.gen() {
                let op = site1.local_insert(rng.gen_range(0, site1.text.len() + 1), s1.next().unwrap());
                println!("site1 {:?}", op);
                site2.apply(op);
            } else {
                let op = site2.local_insert(rng.gen_range(0, site2.text.len() + 1), s2.next().unwrap());
                println!("site2 {:?}", op);
                site1.apply(op);

            }
        }
        assert_eq!(site1.text, site2.text);

    }
}
