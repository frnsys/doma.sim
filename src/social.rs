use fnv::FnvHashSet;
use petgraph::csr::Csr;
use rand::prelude::*;
use rand::rngs::StdRng;

pub struct SocialGraph {
    graph: Csr<usize, ()>,
}

impl SocialGraph {
    pub fn new(n: usize, friend_limit: usize, mut rng: &mut StdRng) -> SocialGraph {
        let graph = Csr::<usize, ()>::with_nodes(n);
        let mut social_graph = SocialGraph {
            graph: graph
        };
        for i in 0..n {
            let n_friends = rng.gen_range(0, friend_limit) as usize;
            social_graph.add_random_friends(i, n_friends, &mut rng);
        }

        social_graph
    }

    pub fn add_random_friends(&mut self, id: usize, n: usize, rng: &mut StdRng) {
        // There may be some redundancy here,
        // which we accept for simplicity
        for _ in 0..n {
            let friend = rng.gen_range(0, self.graph.node_count());
            self.graph.add_edge(id as u32, friend as u32, ());
        }
    }

    pub fn contagion(&self, start_id: usize, p: f32, t: f32, max_depth: usize, rng: &mut StdRng) -> FnvHashSet<usize> {
        let mut depth = 0;
        let mut nodes = FnvHashSet::default();
        let mut next = FnvHashSet::default();
        let mut fringe = FnvHashSet::default();

        fringe.insert(start_id);
        while fringe.len() > 0 {
            next.clear();
            for id in fringe.drain() {
                let neighbs = self.graph.neighbors_slice(id as u32);
                for n in neighbs {
                    let n = *n as usize;

                    // Don't revisit nodes
                    if !nodes.contains(&n) {
                        // Do the two encounter each other?
                        let roll_p: f32 = rng.gen();

                        // Does the other person become infected?
                        let roll_t: f32 = rng.gen();

                        if roll_p < p && roll_t < t {
                            nodes.insert(n);
                            next.insert(n);
                        }
                    }
                }
            }
            fringe = next.iter().cloned().collect();
            depth += 1;
            if depth >= max_depth {
                break;
            }
        }
        nodes
    }
}
