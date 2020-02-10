pub mod ident;

use ident::*;
use serde::{Deserialize, Serialize};

// identifier, clock, site id, char
type Entry = (Identifier, u64, u32, char);

#[derive(Serialize, Deserialize)]
pub struct LSeq {
    text: Vec<Entry>, // gen: NameGenerator
    gen: IdentGen,
    clock: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Op {
    Insert {
        #[serde(flatten)]
        id: Identifier,
        clock: u64,
        site_id: u32,
        c: char
    },
    Delete{
        remote: (u32, u64),
        #[serde(flatten)]
        id: Identifier,
        site_id: u32,
        clock: u64
    },
}

impl LSeq {
    pub fn new(id: u32) -> Self {
        LSeq { text: Vec::new(), gen: IdentGen::new(id), clock: 0 }
    }
    pub fn do_insert(&mut self, ix: Identifier, clock: u64, site_id: u32, c: char) {
        // Inserts only have an impact if the identifier is in the tree
        if let Err(res) = self.text.binary_search_by(|e| e.0.cmp(&ix)) {
            self.text.insert(res, (ix, clock, site_id, c));
        }
    }

    pub fn do_delete(&mut self, ix: &Identifier) {
        // Deletes only have an effect if the identifier is already in the tree
        if let Ok(i) = self.text.binary_search_by(|e| e.0.cmp(&ix)) {
            self.text.remove(i);
        }
    }

    pub fn apply(&mut self, op: &Op){
        match op {
            Op::Insert{id, clock, site_id, c} => self.do_insert(id.clone(), *clock, *site_id, *c),
            Op::Delete{id,..} => self.do_delete(id),
        }
    }

    // Perform a local insertion and create the operation that should be broadcast
    pub fn local_insert(&mut self, ix: usize, c: char) -> Op {
        let lower = self.gen.lower();
        let upper = self.gen.upper();
        // append!
        let ix_ident = if self.text.len() <= ix {
            let prev = self.text.last().map(|(i, _, _, _)| i).unwrap_or_else(|| &lower);
            self.gen.alloc(prev, &upper)
        } else {
            let prev = match ix.checked_sub(1) {
                Some(i) => &self.text.get(i).unwrap().0,
                None => &lower,
            };
            let next = self.text.get(ix).map(|(i, _, _, _)| i).unwrap_or(&upper);
            let a = self.gen.alloc(&prev, next);

            assert!(prev < &a);
            assert!(&a < next);
            a
        };
        let op = Op::Insert{ id: ix_ident, clock: self.clock, site_id: self.gen.site_id, c };
        self.clock += 1;
        self.apply(&op);
        op


    }

    pub fn local_delete(&mut self, ix: usize) -> Op {
        let data = self.text[ix].clone();

        // // bug!! should be the
        let op = Op::Delete{ id: data.0, remote: (data.2, data.1), clock: self.clock, site_id: self.gen.site_id };

        // let ident = self.text[ix].0.clone();
        // let op = Op::Delete{ id: ident, clock: self.clock, site_id: self.gen.site_id };

        self.clock += 1;

        self.apply(&op);
        op

    }

    pub fn text(&self) -> String {
        self.text.iter().map(|(_, _, _, c,)| c).collect::<String>()
    }

    pub fn raw_text(&self) -> & Vec<Entry> {
        &self.text
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::distributions::Alphanumeric;
    use rand::Rng;

    #[test]
    fn test_out_of_order_inserts() {
        let mut site1 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, 0), clock: 0 };
        //let mut site2 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, 1) };
        site1.local_insert(0, 'a');
        site1.local_insert(1, 'c');
        site1.local_insert(1, 'b');

        assert_eq!(site1.text(), "abc");
    }

    #[test]
    fn test_inserts() {
        // A simple smoke test to ensure that insertions work properly.
        // Uses two sites which random insert a character and then immediately insert it into the
        // other site.
        let mut rng = rand::thread_rng();

        let mut s1 = rng.sample_iter(Alphanumeric);
        let mut s2 = rng.sample_iter(Alphanumeric);

        let mut site1 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, 0), clock: 0 };
        let mut site2 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, 1), clock: 0 };

        for _ in 0..5000 {
            if rng.gen() {
                let op = site1.local_insert(rng.gen_range(0, site1.text.len() + 1), s1.next().unwrap());
                site2.apply(&op);
            } else {
                let op = site2.local_insert(rng.gen_range(0, site2.text.len() + 1), s2.next().unwrap());
                site1.apply(&op);
            }
        }
        assert_eq!(site1.text, site2.text);
    }

    #[derive(Clone)]
    struct OperationList(pub Vec<Op>);

    use quickcheck::{Arbitrary, Gen};

    impl Arbitrary for OperationList {
        fn arbitrary<G: Gen>(g: &mut G) -> OperationList {
            let size = {
                let s = g.size();
                if s == 0 {
                    0
                } else {
                    g.gen_range(0, s)
                }
            };

            let mut site1 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, g.gen()), clock: 0 };
            let ops = (0..size)
                .map(|_| {
                    if g.gen() || site1.text.len() == 0 {
                        site1.local_insert(g.gen_range(0, site1.text.len() + 1), g.gen())
                    } else {
                        site1.local_delete(g.gen_range(0, site1.text.len()))
                    }
                })
                .collect::<Vec<Op>>();
            OperationList(ops)
        }
        // implement shrinking ://
    }

    #[test]
    fn prop_inserts_and_deletes() {
        let mut rng = quickcheck::StdThreadGen::new(1000);
        let mut op1 = OperationList::arbitrary(&mut rng).0.into_iter();
        let mut op2 = OperationList::arbitrary(&mut rng).0.into_iter();

        let mut site1 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, 0), clock: 0};
        let mut site2 = LSeq { text: Vec::new(), gen: IdentGen::new_with_args(INITIAL_BASE, 1), clock: 0};

        let mut s1_empty = false;
        let mut s2_empty = false;
        while !s1_empty && !s2_empty {
            if rng.gen() {
                match op1.next() {
                    Some(o) => {
                        site1.apply(&o);
                        site2.apply(&o);
                    }
                    None => {
                        s1_empty = true;
                    }
                }
            } else {
                match op2.next() {
                    Some(o) => {
                        site1.apply(&o);
                        site2.apply(&o);
                    }
                    None => {
                        s2_empty = true;
                    }
                }
            }
        }

        assert_eq!(site1.text, site2.text);
    }
}
