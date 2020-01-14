use rand::Rng;

const BOUNDARY: u64 = 10;

#[derive(Debug)]
struct Identifier {
    path: Vec<usize>,
}

fn prefix(x: Identifier, depth: usize) -> Identifier {
    let mut idCopy = Vec::new();
    for cpt in 0..depth {
        if cpt < x.path.len() {
            idCopy.push(x.path[cpt])
        } else {
            idCopy.push(0);
        }
    }
    return Identifier { path: idCopy };
}

/// This function counts the amount of names between two identifiers when we look at a specific
/// depth. The difficulty is that each layer of the tree is exponentially larger than the previous
fn interval(a: Identifier, b: Identifier, depth: usize, orig_base: u32) -> usize {
    // Terminology: an active node is a node in the tree that is along one of our paths.
    // A free node is a node not in the path but within our selected interval.
    //
    // Sum stores the free nodes we've found 
    let mut sum: usize = 0;
    let mut is_prefix: bool = true;
    
    for ix in 0..depth {
        let base: usize = 2usize.pow(ix as u32 + orig_base); 
        
        // We give every free node at the previous level it's full set of children.
        sum *= base;
        // Fetch the next segment of each path

        let b_ix = match b.path.get(ix) {
            Some(b) => &b,
            // The right segment is shorter so we count every node as free by assigning it the
            // largest value allowed.
            None => &base 

        };
        let mut adjust = 0;
        let a_ix = match a.path.get(ix) {
            Some(a) => a,
            // The left segment is shorter so we count every node as free and setting [adjust] to 1
            // to count the 0th index as free as well.
            None => { adjust = 1; &0},
        };
        
        if is_prefix && a_ix > b_ix  {
            panic!("this range is flipped")
        }

        // Here we look at the active nodes at depth ix. We need to add the childen of these node
        // that are free.
        // There are two basic cases:
        // 1. The paths have shared a common trunk until now.
        //          parent
        //            |
        //      +-+-+-+-+-+-+-+ 
        //      | | | | | | | | 
        //      v v v v v v v v
        //        a ..... b  ^
        //            ^   ^  |
        //            |   |  +--- dead nodes to eliminate 
        //            |   +----- active nodes (for current segment
        //            +---- live nodes to add
        //   From this diagram we can see that the amount of free nodes to add is b - a - 1 (to
        //   exclude a from the range)
        // 2. In the second case, the paths already diverged in the past so they don't share a
        //    parent.
        //        parent1  .........  parent2     
        //          |          ^        |
        //    +-+-+-+-+-+-+-+  |  +-+-+-+-+-+-+-+
        //    | | | | | | | |  |  | | | | | | | |
        //    v v v v v v v v  |  v v v v v v v v
        //      a ..........)  |  (... b 
        //              ^      |    ^ 
        //              |      +----|--------------- free nodes already added at previous iterations
        //              +-----------+--------------- the free nodes we need to calculate here.
        //
        //  Here a and b have different parents. We have already included all the other subtrees
        //  inbetween the parents of a and b since they must have been free at the previous
        //  iteration. So here we just need to figure out the free range of parent1 and parent2.
        //  This leads to the calculation:
        //
        //      (max_nodes_at_level - a - 1) + b

                
        
        if !is_prefix {
            sum += base - (a_ix + 1) + b_ix;
        } else if b_ix != a_ix {
            sum += b_ix - a_ix - 1;
        }
        sum += adjust;
        
        // Are both paths still sharing a common trunc? 
        if b_ix != a_ix {
            is_prefix = false;
        }
    }

    return sum;
}
#[test]
fn test_prefix() {
    let x = Identifier { path: vec![1, 0] };
    let y = Identifier { path: vec![1, 15] };

    assert_eq!(interval(x, y, 2, 3), 14);

    let a = Identifier { path: vec![1, 0] };
    let b = Identifier { path: vec![3, 15] };

    assert_eq!(interval(a, b, 2, 3), 46);

    let c = Identifier { path: vec![1, 15] };
    let d = Identifier { path: vec![3, 0] };

    assert_eq!(interval(c, d, 2, 3), 16);

}
#[test]
fn test_different_len_paths() {
    let x = Identifier { path: vec![1] };
    let y = Identifier { path: vec![1, 15] };

    assert_eq!(interval(x, y, 2, 3), 15);
}

#[test]
fn test_long_paths() {
    let x = Identifier {
        path: vec![1, 0, 1],
    };
    let y = Identifier {
        path: vec![3, 1, 0],
    };

    assert_eq!(interval(x, y, 3, 3), 30 + 32 * 32);
}
// fn alloc(p: Identifier, q: Identifier) -> Identifier { // make that a bigint
//     let mut depth = 0;
//     let mut interval = 0;
//
//     while (interval < 1) {
//         depth += 1;
//
//         //interval = prefix(q, depth) - prefix(p, depth) - 1;
//     }
//
//     let step = u64::min(BOUNDARY, interval);
//     let mut rng = rand::thread_rng();
//     let shift = rng.gen_range(0, step) + 1;
//
//     if (depth % 2 == 0) { // hash
//         return prefix(p, depth) + shift;
//     } else {
//         return prefix(q, depth) - shift;
//     }
// }
