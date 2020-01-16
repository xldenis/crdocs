use rand::Rng;

const BOUNDARY: usize = 10;

const INITIAL_BASE: u32 = 3; // start with 2^8

#[derive(Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct Identifier {
    path: Vec<usize>,
    site_id : u64,
}

pub struct IdentGen {
    initial_base_bits: u32,
    site_id : u64,
}

impl IdentGen {
    pub fn new() -> IdentGen {
        IdentGen {
            initial_base_bits: INITIAL_BASE,
            site_id: 0,
        }
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

                        return Identifier { path: path_, site_id: self.site_id }
                    } 
                }

                // The upper bound ended on the previous level.
                // This means we can allocate a node in the range a..max_for_depth
                (Some(a), None)    => {
                    // largest index at the current depth
                    let max_for_depth = 2usize.pow(self.initial_base_bits + depth as u32) - 1;

                    let next_index = self.index_in_range(*a, max_for_depth, depth as u32);
                    let mut path_ = q.path.clone();
                    path_.push(next_index);

                    return Identifier { path: path_, site_id: self.site_id }

                }
                // The lower bound ended on the previous level which means we can allocate in the
                // rnage 0..b
                (None, Some(b))    => {
                    let next_index = self.index_in_range(0, *b, depth as u32);
                    let mut path_ = p.path.clone();
                    path_.push(next_index);

                    return Identifier { path: path_, site_id: self.site_id }
                }
                // The two paths are fully equal which means that the site_ids MUST be different or
                // we are in an invalid situation
                (None, None)       => {
                    assert!(p.site_id != q.site_id, "can't allocate between identical identifiers! {} {}", p.site_id, q.site_id);
                    let max_for_depth = 2usize.pow(self.initial_base_bits + depth as u32) - 1;
                    let next_index = self.index_in_range(0, max_for_depth, depth as u32);
                    let mut path_ = p.path.clone();
                    path_.push(next_index);

                    return Identifier { path: path_, site_id: self.site_id }
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
        assert!(lower < upper, "");

        let mut rng = rand::thread_rng();
        let interval = BOUNDARY.min(upper - lower); 
        assert!(interval > 0, "range of valid ids is zero");

        let step = rng.gen_range(1, interval);
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

    let c = Identifier {
        path: vec![1, 0, 1],
        site_id: 0,
    };
    let d = Identifier {
        path: vec![1, 0, 3],
        site_id: 0,
    };

    assert_eq!(
        gen.alloc(&c, &d),
        Identifier {
            path: vec![1, 0, 2],
            site_id: 0,
        }
    );
}
}

pub struct LSeq {
    text: Vec<(Identifier, u64)>, // gen: NameGenerator
}

impl LSeq {
    pub fn do_insert(&mut self, ix: Identifier, c: u64) {
        let res = self.text.binary_search_by(|e| e.0.cmp(&ix));

        match res {
            // The index already exists! in which case site_id is the tie breaker!
            Ok(i) => {
            }
            // The index doesn't exist in our current text
            Err(i) => {
                // the index we want to insert is outside the current range
                if self.text.len() <= i {
                    self.text.push((ix, c))
                }
            }
        }
    }
}
