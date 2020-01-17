use rand::Rng;

use rand::distributions::Alphanumeric;

const BOUNDARY: usize = 10;

const INITIAL_BASE: u32 = 3; // start with 2^8

#[derive(Debug, PartialEq, PartialOrd, Ord, Eq, Clone)]
pub struct Identifier {
    path: Vec<usize>,
    site_id: u64,
}

pub struct IdentGen {
    initial_base_bits: u32,
    site_id: u64,
}

impl IdentGen {
    pub fn new() -> IdentGen {
        Self::new_with_args(INITIAL_BASE, 0)
    }

    pub fn new_with_args(base: u32, site_id: u64) -> IdentGen {
        IdentGen { initial_base_bits: INITIAL_BASE, site_id: site_id }
    }

    pub fn lower(&self) -> Identifier {
        Identifier { path: vec![0], site_id: self.site_id }
    }

    pub fn upper(&self) -> Identifier {
        Identifier { path: vec![2usize.pow(self.initial_base_bits) - 1], site_id: self.site_id }
    }
    /// Allocates a new identifier between p and q.
    /// Requires that p < q and will produce a new identifier z, p < z < q
    pub fn alloc(&mut self, p: &Identifier, q: &Identifier) -> Identifier {
        assert!(p <= q, "lower bound greater than upper bound!");
        let mut depth = 0;

        // Descend both identifiers in parallel until we find room to insert an identifier
        loop {
            match (p.path.get(depth), q.path.get(depth)) {
                // Our descent continues...
                (Some(a), Some(b)) => {
                    // a and b have diverged, which means there's room to insert an identifier!
                    if a != b {
                        assert!(a < b, "lower bound at depth {:?} greater than upper bound!", depth);

                        // Generate a new index between a and b.
                        let next_index = self.index_in_range(*a, *b, depth as u32);
                        let mut path_ = p.path.clone();
                        path_.truncate(depth);
                        path_.push(next_index);

                        return Identifier { path: path_, site_id: self.site_id };
                    }
                }

                // The upper bound ended on the previous level.
                // This means we can allocate a node in the range a..max_for_depth
                (Some(a), None) => {
                    // largest index at the current depth
                    let max_for_depth = 2usize.pow(self.initial_base_bits + depth as u32) - 1;

                    let next_index = self.index_in_range(*a, max_for_depth, depth as u32);
                    let mut path_ = q.path.clone();
                    path_.push(next_index);

                    return Identifier { path: path_, site_id: self.site_id };
                }
                // The lower bound ended on the previous level which means we can allocate in the
                // rnage 0..b
                (None, Some(b)) => {
                    let next_index = self.index_in_range(0, *b, depth as u32);
                    let mut path_ = p.path.clone();
                    path_.push(next_index);

                    return Identifier { path: path_, site_id: self.site_id };
                }
                // The two paths are fully equal which means that the site_ids MUST be different or
                // we are in an invalid situation
                (None, None) => {
                    assert!(
                        p.site_id != q.site_id,
                        "can't allocate between identical identifiers! {} {}",
                        p.site_id,
                        q.site_id
                    );
                    let max_for_depth = 2usize.pow(self.initial_base_bits + depth as u32) - 1;
                    let next_index = self.index_in_range(0, max_for_depth, depth as u32);
                    let mut path_ = p.path.clone();
                    path_.push(next_index);

                    return Identifier { path: path_, site_id: self.site_id };
                }
            };
            depth += 1;
        }
    }

    // Generate an index in a given range at the specified depth.
    // Uses the allocation strategy of that depth, boundary+ or boundary- which is biased to the
    // lower and upper ends of the range respectively.
    //
    fn index_in_range(&mut self, lower: usize, upper: usize, depth: u32) -> usize {
        assert!(lower + 1 < upper, "need at least one space between the bounds lower={} upper={}", lower, upper);

        let mut rng = rand::thread_rng();
        let interval = BOUNDARY.min(upper - lower);
        assert!(interval > 0, "range of valid ids is zero");

        let step = rng.gen_range(1, interval);
        assert!(step != 0, "step can't be empty lower={} upper={}", lower, upper);
        if self.strategy(depth) {
            //boundary+

            lower + step
        } else {
            //boundary-

            upper - step
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
                panic!("tried to insert an index that's already in!! {:?}", ix);
            }
            // The index doesn't exist in our current text
            Err(i) => {
                // the index we want to insert is outside the current range
                self.text.insert(i, (ix, c));
            }
        }
    }

    pub fn do_delete(&mut self, ix: Identifier) {
        let res = self.text.binary_search_by(|e| e.0.cmp(&ix));
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
        let prev = self.text.get(ix).map(|(i, _)| i).unwrap_or_else(|| &lower);
        let next = self.text.get(ix + 1).map(|(i, _)| i).unwrap_or_else(|| &upper);

        let ix_ident = self.gen.alloc(prev, next);

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

    #[test]
    fn test_alloc_eq_path() {
        let mut gen = IdentGen::new();

        let x = Identifier { path: vec![1, 0], site_id: 0 };
        let y = Identifier { path: vec![1, 0], site_id: 1 };

        let b = gen.alloc(&x, &y);
    }

    #[test]
    fn test_different_len_paths() {
        let mut gen = IdentGen::new();
        let x = Identifier { path: vec![1], site_id: 0 };
        let y = Identifier { path: vec![1, 15], site_id: 0 };

        let z = gen.alloc(&x, &y);

        assert!(x < z);
        assert!(z < y);
    }

    #[test]
    fn test_alloc() {
        let mut gen = IdentGen::new();
        let a = Identifier { path: vec![1], site_id: 0 };
        let b = Identifier { path: vec![3], site_id: 0 };

        assert_eq!(gen.alloc(&a, &b), Identifier { path: vec![2], site_id: 0 });

        let c = Identifier { path: vec![1, 0, 1], site_id: 0 };
        let d = Identifier { path: vec![1, 0, 3], site_id: 0 };

        assert_eq!(gen.alloc(&c, &d), Identifier { path: vec![1, 0, 2], site_id: 0 });

        let e = Identifier { path: vec![1], site_id: 0 };
        let f = Identifier { path: vec![2], site_id: 0 };

        let res = gen.alloc(&e, &f); 

        assert!(e < res);
        assert!(res < f);

    }


    #[test]
    fn test_inserts () {
        // A simple smoke test to ensure that insertions work properly. 
        // Uses two sites which random insert a character and then immediately insert it into the
        // other site.
        let mut rng = rand::thread_rng();
        let mut s1 = "omgomgomgomg".chars().peekable();
        let mut s2 = "lalalalalala".chars().peekable();
        let mut site1 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, 0) };
        let mut site2 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, 1) };
         
        for _ in 0..24 {
            if (rng.gen() && s1.peek().is_some()) || s2.peek().is_none() {
                let op = site1.local_insert(rng.gen_range(0, site1.text.len() + 1), s1.next().unwrap());
                println!("site1 {:?}", op);
                site2.apply(op);
            } else {
                let op = site2.local_insert(rng.gen_range(0, site2.text.len() + 1), s2.next().unwrap());
                println!("site2 {:?}", op);
                site1.apply(op);
                    
            }
        }

       
    }
}
