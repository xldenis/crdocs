use rand::Rng;
use serde::{Deserialize, Serialize};

const BOUNDARY: u64 = 10;

pub const INITIAL_BASE: u32 = 3; // start with 2^8

#[derive(Debug, PartialEq, PartialOrd, Ord, Eq, Clone, Serialize, Deserialize, Hash)]
pub struct Identifier {
    path: Vec<(u64, u32)>,
    // site_id: u64,
    // counter: u64
}

#[derive(Serialize, Deserialize)]
pub struct IdentGen {
    initial_base_bits: u32,
    site_id: u32,
    // clock: u64
}

impl IdentGen {
    pub fn new(site_id: u32) -> IdentGen {
        Self::new_with_args(INITIAL_BASE, site_id)
    }

    pub fn new_with_args(base: u32, site_id: u32) -> IdentGen {
        IdentGen { initial_base_bits: base, site_id: site_id }
    }

    pub fn lower(&self) -> Identifier {
        Identifier { path: vec![(0, 0)] }
    }

    pub fn upper(&self) -> Identifier {
        Identifier { path: vec![(2u64.pow(self.initial_base_bits) - 1, 0)] }
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
                (Some(a), Some(b)) if a == b => {}
                // We found a gap between the two identifiers
                (Some(a), Some(b)) if a.0 + 1 < b.0 => {
                    // great! let's allocate an identifier at this depth then
                    let next_index = self.index_in_range(a.0 + 1, b.0, depth as u32);
                    return self.replace_last(p, depth, next_index);
                }

                // The upper bound ended on the previous level.
                // This means we can allocate a node in the range a..max_for_depth
                // If there isn't room at this level because _a = max_for_depth then we'll keep
                // searching below.
                (Some(_a), _) => {
                    return self.alloc_with_lower(p, depth + 1);
                }
                // Because the upper bound is zero, we're forced to create a path with zeros until
                // the upper bound is either empty or non-zero
                (None, Some(_b)) => {
                    return self.alloc_with_upper(p, q, depth);
                }

                // The two paths are fully equal which means that the site_ids MUST be different or
                // we are in an invalid situation
                (None, None) => {
                    let max_for_depth = 2u64.pow(self.initial_base_bits + depth as u32) - 1;
                    let next_index = self.index_in_range(1, max_for_depth, depth as u32);
                    return self.push_index(p, next_index);
                }
            };
            depth += 1;
        }
    }

    // Here the problem is that we've run out of the lower path and the upper one is zero!
    // The idea is to keep pushing 0s onto the lower path until we can find a new level to allocate at.
    fn alloc_with_upper(&mut self, p: &Identifier, q: &Identifier, mut depth: usize) -> Identifier {
        let mut ident = p.clone();
        loop {
            match q.path.get(depth) {
                // append a 0 to the result path as well
                Some(b) if b.0 <= 1 => ident.path.push((0, b.1)),
                // oo! a non-zero value
                _ => break,
            }
            depth += 1;
        }

        // If we actually ran out of upper bound values then we're free to choose
        // anything on the next level, otherwise use the upper bound we've found.
        let upper = match q.path.get(depth) {
            Some(b) => b.0,
            None => self.width_at(depth + 1),
        };
        let next_index = self.index_in_range(1, upper, depth as u32);
        return self.push_index(&ident, next_index);
    }

    // Here we have the lowest possible upper bound and we just need to traverse the lower bound
    // until we can find somehwere to insert a new identifier
    fn alloc_with_lower(&mut self, p: &Identifier, depth: usize) -> Identifier {
        let mut lower_bound = depth;

        loop {
            match p.path.get(lower_bound) {
                Some((ix, _)) if ix + 1 < self.width_at(lower_bound) => {
                    let next_index = self.index_in_range(*ix + 1, self.width_at(lower_bound), lower_bound as u32);
                    return self.push_index(p, next_index);
                }
                Some(_) => {}
                None => {
                    let next_index = self.index_in_range(1, self.width_at(lower_bound), lower_bound as u32);
                    return self.push_index(p, next_index);
                }
            }
            lower_bound += 1;
        }
    }

    fn replace_last(&mut self, p: &Identifier, depth: usize, ix: u64) -> Identifier {
        let mut ident = p.clone();
        ident.path.truncate(depth);
        ident.path.push((ix, self.site_id));
        return ident;
    }

    fn push_index(&mut self, p: &Identifier, ix: u64) -> Identifier {
        let mut ident = p.clone();
        ident.path.push((ix, self.site_id));
        return ident;
    }

    fn width_at(&self, depth: usize) -> u64 {
        2u64.pow(self.initial_base_bits + depth as u32)
    }
    // Generate an index in a given range at the specified depth.
    // Uses the allocation strategy of that depth, boundary+ or boundary- which is biased to the
    // lower and upper ends of the range respectively.
    // should allocate in the range [lower, upper)
    fn index_in_range(&mut self, lower: u64, upper: u64, depth: u32) -> u64 {
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

#[cfg(test)]
mod test {
    use super::*;
    use quickcheck::{Arbitrary, Gen, TestResult};

    impl Arbitrary for Identifier {
        fn arbitrary<G: Gen>(g: &mut G) -> Identifier {
            Identifier { path: Vec::<(u64, u32)>::arbitrary(g) }
        }
    }

    #[quickcheck]
    fn prop_alloc(p: Identifier, q: Identifier) -> TestResult {
        if p >= q || p.path.len() == 0 || q.path.len() == 0 {
            return TestResult::discard();
        }
        let mut gen = IdentGen::new(0);
        let z = gen.alloc(&p, &q);

        TestResult::from_bool(p < z && z < q)
    }

    #[test]
    fn test_alloc_eq_path() {
        let mut gen = IdentGen::new(0);

        let x = Identifier { path: vec![(1, 0), (1, 0)] };
        let y = Identifier { path: vec![(1, 0), (1, 1)] };
        gen.alloc(&x, &y);
        let b = gen.alloc(&x, &y);
        // println!("{:?} {:?} {:?}", x, b, y);
        assert!(x < b);
        assert!(b < y);
    }

    #[test]
    fn test_different_len_paths() {
        let mut gen = IdentGen::new(0);
        let x = Identifier { path: vec![(1, 0)] };
        let y = Identifier { path: vec![(1, 0), (15, 0)] };

        let z = gen.alloc(&x, &y);

        assert!(x < z);
        assert!(z < y);
    }

    #[test]
    fn test_alloc() {
        let mut gen = IdentGen::new(0);
        let a = Identifier { path: vec![(1, 0)] };
        let b = Identifier { path: vec![(3, 0)] };

        assert_eq!(gen.alloc(&a, &b), Identifier { path: vec![(2, 0)] });

        let c = Identifier { path: vec![(1, 0), (0, 0), (1, 0)] };
        let d = Identifier { path: vec![(1, 0), (0, 0), (3, 0)] };

        assert_eq!(gen.alloc(&c, &d), Identifier { path: vec![(1, 0), (0, 0), (2, 0)] });

        let e = Identifier { path: vec![(1, 0)] };
        let f = Identifier { path: vec![(2, 0)] };

        let res = gen.alloc(&e, &f);

        assert!(e < res);
        assert!(res < f);
        {
            let mut gen = IdentGen::new_with_args(INITIAL_BASE, 1);

            let a = Identifier { path: vec![(4, 0), (4, 0)] };
            let b = Identifier { path: vec![(4, 0), (4, 0), (1, 1)] };
            // let a = Identifier { path: vec![(4, 0)]};
            // let b = Identifier { path: vec![(4, 1)]};

            let c = gen.alloc(&a, &b);
            println!("{:?}", c);
            assert!(a < c);
            assert!(c < b);
        }
        {
            let a = Identifier { path: vec![(5, 1), (6, 1), (6, 1), (6, 0)] };
            let b = Identifier { path: vec![(5, 1), (6, 1), (6, 1), (6, 0), (0, 0), (507, 0)] };

            let c = gen.alloc(&a, &b);
            println!("{:?}", c);
            assert!(a < c);
            assert!(c < b);
        }
    }

    #[test]
    fn test_index_in_range() {
        let mut gen = IdentGen::new(0);
        assert_eq!(gen.index_in_range(0, 1, 1), 0);
    }
}
