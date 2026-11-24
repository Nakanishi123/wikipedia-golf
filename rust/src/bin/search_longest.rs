use clap::Parser;
use num24::u24;
use rayon::prelude::*;
use serde::Serialize;
use std::error::Error;
use std::io::BufWriter;
use wikipedia_golf::structs::Page;
use wikipedia_golf::util::load_bincode_zst;

const BATCH_SIZE: usize = 64;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, value_name = "PATH", help = "path to pages.bin.zst")]
    pages_bin: String,
    #[arg(long, value_name = "PATH", help = "path to page_edges.bin.zst")]
    page_edges_bin: String,
    #[arg(long, value_name = "NUMBER", help = "matrix num")]
    matrix_num: usize,
    #[arg(long, value_name = "NUMBER", help = "matrix size")]
    matrix_size: usize,
    #[arg(long, short = 'o', value_name = "PATH", help = "output file")]
    output_file: String,
}

#[derive(Serialize)]
struct LongestPathResult {
    id: u32,
    path: Vec<u32>,
    distance_count: Vec<u32>,
}

#[derive(Debug)]
struct CsrGraph {
    offsets: Vec<u32>,
    edges: Vec<u32>,
}

impl CsrGraph {
    #[inline(always)]
    fn neighbors(&self, node: u32) -> &[u32] {
        let i = node as usize;
        let start = self.offsets[i] as usize;
        let end = self.offsets[i + 1] as usize;
        &self.edges[start..end]
    }

    fn build_reverse(&self, n: usize) -> Self {
        let mut counts = vec![0u32; n];
        for &to in &self.edges {
            counts[to as usize] += 1;
        }
        let mut offsets = Vec::with_capacity(n + 1);
        let mut sum = 0u32;
        offsets.push(0);
        for &c in &counts {
            sum += c;
            offsets.push(sum);
        }
        let mut cursor = offsets[..n].to_vec();
        let mut edges = vec![0u32; self.edges.len()];
        for u in 0..n {
            let start = self.offsets[u] as usize;
            let end = self.offsets[u + 1] as usize;
            let u32_u = u as u32;
            for &v in &self.edges[start..end] {
                let pos = cursor[v as usize] as usize;
                edges[pos] = u32_u;
                cursor[v as usize] += 1;
            }
        }
        Self { offsets, edges }
    }
}

struct BidirWorkspace {
    fwd_gen: Vec<u32>,
    rev_gen: Vec<u32>,
    fwd_parent: Vec<u32>,
    rev_parent: Vec<u32>,
    fwd_q: Vec<u32>,
    rev_q: Vec<u32>,
    gen: u32,
}

impl BidirWorkspace {
    fn new(n: usize) -> Self {
        Self {
            fwd_gen: vec![0; n],
            rev_gen: vec![0; n],
            fwd_parent: vec![0; n],
            rev_parent: vec![0; n],
            fwd_q: Vec::with_capacity(4096),
            rev_q: Vec::with_capacity(4096),
            gen: 0,
        }
    }

    fn reconstruct_path(
        &mut self,
        fwd: &CsrGraph,
        rev: &CsrGraph,
        page_ids: &[u32],
        start: u32,
        target: u32,
        expected_dist: usize,
    ) -> Vec<u32> {
        if expected_dist == 0 {
            return vec![page_ids[start as usize]];
        }
        if expected_dist == 1 {
            return vec![page_ids[start as usize], page_ids[target as usize]];
        }

        const ROOT: u32 = u32::MAX;
        self.gen = self.gen.wrapping_add(1);
        if self.gen == 0 {
            self.fwd_gen.fill(0);
            self.rev_gen.fill(0);
            self.gen = 1;
        }
        let gen = self.gen;

        self.fwd_q.clear();
        self.rev_q.clear();

        self.fwd_q.push(start);
        self.fwd_gen[start as usize] = gen;
        self.fwd_parent[start as usize] = ROOT;

        self.rev_q.push(target);
        self.rev_gen[target as usize] = gen;
        self.rev_parent[target as usize] = ROOT;

        let mut fwd_head = 0usize;
        let mut rev_head = 0usize;

        let (meet_u, meet_v) = loop {
            let fwd_size = self.fwd_q.len() - fwd_head;
            let rev_size = self.rev_q.len() - rev_head;

            if fwd_size <= rev_size {
                let level_end = self.fwd_q.len();
                let mut found = None;
                'fwd_loop: while fwd_head < level_end {
                    let u = self.fwd_q[fwd_head];
                    fwd_head += 1;
                    for &v in fwd.neighbors(u) {
                        if self.rev_gen[v as usize] == gen {
                            found = Some((u, v));
                            break 'fwd_loop;
                        }
                        if self.fwd_gen[v as usize] != gen {
                            self.fwd_gen[v as usize] = gen;
                            self.fwd_parent[v as usize] = u;
                            self.fwd_q.push(v);
                        }
                    }
                }
                if let Some(pair) = found {
                    break pair;
                }
            } else {
                let level_end = self.rev_q.len();
                let mut found = None;
                'rev_loop: while rev_head < level_end {
                    let v = self.rev_q[rev_head];
                    rev_head += 1;
                    for &u in rev.neighbors(v) {
                        if self.fwd_gen[u as usize] == gen {
                            found = Some((u, v));
                            break 'rev_loop;
                        }
                        if self.rev_gen[u as usize] != gen {
                            self.rev_gen[u as usize] = gen;
                            self.rev_parent[u as usize] = v;
                            self.rev_q.push(u);
                        }
                    }
                }
                if let Some(pair) = found {
                    break pair;
                }
            }
        };

        let mut path = Vec::with_capacity(expected_dist + 1);
        let mut cur = meet_u;
        while cur != ROOT {
            path.push(page_ids[cur as usize]);
            cur = self.fwd_parent[cur as usize];
        }
        path.reverse();

        let mut cur = meet_v;
        while cur != ROOT {
            path.push(page_ids[cur as usize]);
            cur = self.rev_parent[cur as usize];
        }

        debug_assert_eq!(path.len(), expected_dist + 1);
        path
    }
}

struct BatchBfsWorkspace {
    visited: Vec<u64>,
    frontier: Vec<u64>,
    next_frontier: Vec<u64>,
    active_nodes: Vec<u32>,
    next_active: Vec<u32>,
    bidir: BidirWorkspace,
}

impl BatchBfsWorkspace {
    fn new(n: usize) -> Self {
        Self {
            visited: vec![0; n],
            frontier: vec![0; n],
            next_frontier: vec![0; n],
            active_nodes: Vec::with_capacity(n),
            next_active: Vec::with_capacity(n),
            bidir: BidirWorkspace::new(n),
        }
    }
}

#[inline(always)]
fn add_bit_vertical(acc: &mut [u64; 24], mut carry: u64) {
    for bit in acc.iter_mut() {
        let next_carry = *bit & carry;
        *bit ^= carry;
        carry = next_carry;
        if carry == 0 {
            break;
        }
    }
}

fn search_longest_batch(
    fwd: &CsrGraph,
    rev: &CsrGraph,
    page_ids: &[u32],
    starts: &[u32],
    ws: &mut BatchBfsWorkspace,
) -> Vec<LongestPathResult> {
    let n = page_ids.len();
    let k_len = starts.len();
    let dense_threshold = n / 20;

    ws.visited.fill(0);
    ws.active_nodes.clear();
    ws.next_active.clear();

    let mut distance_counts: Vec<Vec<u32>> = (0..k_len)
        .map(|_| {
            let mut v = Vec::with_capacity(32);
            v.push(1);
            v
        })
        .collect();
    let mut farthest = [0u32; BATCH_SIZE];
    let mut active_mask = 0u64;

    for (k, &s) in starts.iter().enumerate() {
        let bit = 1u64 << k;
        active_mask |= bit;
        farthest[k] = s;
        if ws.frontier[s as usize] == 0 {
            ws.active_nodes.push(s);
        }
        ws.frontier[s as usize] |= bit;
        ws.visited[s as usize] |= bit;
    }

    while !ws.active_nodes.is_empty() {
        let mut acc = [0u64; 24];
        let mut level_any = 0u64;

        if ws.active_nodes.len() <= dense_threshold {
            // Sparse Top-Down (Push)
            for &u in &ws.active_nodes {
                let f = ws.frontier[u as usize];
                for &v in fwd.neighbors(u) {
                    let unseen = f & !ws.visited[v as usize];
                    if unseen != 0 {
                        let nf = &mut ws.next_frontier[v as usize];
                        if *nf == 0 {
                            ws.next_active.push(v);
                        }
                        *nf |= unseen;
                    }
                }
            }
            for &v in &ws.next_active {
                let newly = ws.next_frontier[v as usize];
                ws.visited[v as usize] |= newly;
                level_any |= newly;
                add_bit_vertical(&mut acc, newly);
            }
        } else {
            // Dense Bottom-Up (Pull)
            for v in 0..n {
                let need = active_mask & !ws.visited[v];
                if need == 0 {
                    continue;
                }
                let mut found = 0u64;
                for &u in rev.neighbors(v as u32) {
                    found |= ws.frontier[u as usize] & need;
                    if found == need {
                        break;
                    }
                }
                if found != 0 {
                    ws.next_frontier[v] = found;
                    ws.visited[v] |= found;
                    ws.next_active.push(v as u32);
                    level_any |= found;
                    add_bit_vertical(&mut acc, found);
                }
            }
        }

        // Clear current frontier
        for &u in &ws.active_nodes {
            ws.frontier[u as usize] = 0;
        }
        ws.active_nodes.clear();

        if level_any == 0 {
            break;
        }

        // Extract per-source level counts from bit-vertical accumulator
        let mut m = level_any;
        while m != 0 {
            let k = m.trailing_zeros() as usize;
            let mut cnt = 0u32;
            for (b, &word) in acc.iter().enumerate() {
                cnt |= (((word >> k) & 1) as u32) << b;
            }
            distance_counts[k].push(cnt);
            m &= m - 1;
        }

        // Record one farthest node per source active at this level
        let mut rem = level_any;
        for &v in ws.next_active.iter().rev() {
            let hit = ws.next_frontier[v as usize] & rem;
            if hit != 0 {
                let mut hm = hit;
                while hm != 0 {
                    let k = hm.trailing_zeros() as usize;
                    farthest[k] = v;
                    hm &= hm - 1;
                }
                rem &= !hit;
                if rem == 0 {
                    break;
                }
            }
        }

        active_mask = level_any;
        std::mem::swap(&mut ws.frontier, &mut ws.next_frontier);
        std::mem::swap(&mut ws.active_nodes, &mut ws.next_active);
    }

    // Reconstruct shortest path for each (start, farthest) pair via bidirectional BFS
    starts
        .iter()
        .enumerate()
        .map(|(k, &start)| {
            let expected_dist = distance_counts[k].len() - 1;
            let path = ws.bidir.reconstruct_path(
                fwd,
                rev,
                page_ids,
                start,
                farthest[k],
                expected_dist,
            );
            LongestPathResult {
                id: page_ids[start as usize],
                path,
                distance_count: std::mem::take(&mut distance_counts[k]),
            }
        })
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let pages: Vec<Page> = load_bincode_zst(&args.pages_bin)?;
    let page_edges: Vec<Vec<u24>> = load_bincode_zst(&args.page_edges_bin)?;

    let n = pages.len();
    let total: usize = page_edges.iter().map(|v| v.len()).sum();
    let mut offsets = Vec::with_capacity(n + 1);
    let mut edges = Vec::with_capacity(total);

    offsets.push(0u32);
    for v in page_edges {
        edges.extend(v.into_iter().map(|x| -> u32 { x.into() }));
        offsets.push(edges.len() as u32);
    }
    let fwd = CsrGraph { offsets, edges };
    let rev = fwd.build_reverse(n);

    // index -> page id
    let page_ids: Vec<u32> = pages.iter().map(|page| page.page_id).collect();
    drop(pages);

    let targets: Vec<u32> = (0..n)
        .filter(|&id| id % args.matrix_size == args.matrix_num)
        .map(|id| id as u32)
        .collect();

    let longest_paths: Vec<LongestPathResult> = targets
        .par_chunks(BATCH_SIZE)
        .map_init(
            || BatchBfsWorkspace::new(n),
            |ws, batch| search_longest_batch(&fwd, &rev, &page_ids, batch, ws),
        )
        .flatten()
        .collect();

    let file = std::fs::File::create(&args.output_file)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer(writer, &longest_paths)?;
    Ok(())
}
