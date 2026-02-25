use clap::Parser;
use num24::u24;
use rayon::prelude::*;
use serde_json::json;
use std::error::Error;
use wikipedia_golf::structs::Page;
use wikipedia_golf::util::load_bincode_zst;

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

#[derive(Debug)]
struct CsrGraph {
    offsets: Vec<usize>,
    edges: Vec<u32>,
}

impl CsrGraph {
    fn neighbors(&self, node: u32) -> &[u32] {
        let start = self.offsets[node as usize];
        let end = self.offsets[node as usize + 1];
        &self.edges[start..end]
    }
}

struct BfsWorkspace {
    queue: Vec<u32>,          // BFSのキュー
    parent: Vec<u32>,         // 経路復元用の親ノード
    visited_gen: Vec<u32>,    // 訪問済み判定(世代の番号が入る)
    gen: u32,                 // そのBFSが何回目か(世代)
    distance_count: Vec<u32>, // 各距離におけるノード数
}

impl BfsWorkspace {
    fn new(n: usize) -> Self {
        Self {
            queue: Vec::with_capacity(n.min(1_000_000)),
            parent: vec![0; n],
            visited_gen: vec![0; n],
            gen: 1,
            distance_count: Vec::with_capacity(128),
        }
    }

    fn next_gen(&mut self) {
        self.gen += 1;
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let pages: Vec<Page> = load_bincode_zst(&args.pages_bin)?;
    let page_edges: Vec<Vec<u24>> = load_bincode_zst(&args.page_edges_bin)?;

    // u32の方が速いので変換
    let page_edges: Vec<Vec<u32>> = page_edges
        .into_iter()
        .map(|v| v.into_iter().map(|x| x.into()).collect())
        .collect();

    // CSR 形式に変換
    let total = page_edges.iter().map(|v| v.len()).sum();
    let mut offsets = Vec::with_capacity(page_edges.len() + 1);
    let mut edges = Vec::with_capacity(total);

    offsets.push(0);
    for v in page_edges {
        edges.extend(v);
        offsets.push(edges.len());
    }
    let edges_csr = CsrGraph { offsets, edges };

    // index -> page id
    let page_ids: Vec<u32> = pages.iter().map(|page| page.page_id).collect();

    let longest_paths = (0..pages.len())
        .into_par_iter()
        .filter(|&id| id % args.matrix_size == args.matrix_num)
        .map_init(
            || BfsWorkspace::new(pages.len()),
            |ws, id| search_longest_path(&edges_csr, &page_ids, id as u32, ws),
        )
        .collect::<Vec<_>>();

    let longest_paths_json = longest_paths
        .into_iter()
        .map(|indices| {
            json!({
                "id": indices.0[0],
                "path": indices.0,
                "distance_count": indices.1,
            })
        })
        .collect::<Vec<_>>();

    let mut file = std::fs::File::create(&args.output_file)?;
    serde_json::to_writer(&mut file, &longest_paths_json)?;
    Ok(())
}

fn search_longest_path(
    g: &CsrGraph,
    page_ids: &[u32],
    start: u32,
    ws: &mut BfsWorkspace,
) -> (Vec<usize>, Vec<u32>) {
    const ROOT: u32 = u32::MAX - 1;

    // BFSの初期化
    ws.next_gen(); // 世代を進める
    ws.queue.clear();
    ws.distance_count.clear();

    // 開始ノードをキューに追加し、親ノードをROOTに設定
    ws.queue.push(start);
    ws.parent[start as usize] = ROOT;
    ws.visited_gen[start as usize] = ws.gen;

    let gen = ws.gen;
    let mut head = 0;
    let mut level_start = 0; // 現在の距離レベルの開始位置
    let mut farthest = start;

    // BFSループ
    while level_start < ws.queue.len() {
        let level_end = ws.queue.len();
        ws.distance_count.push((level_end - level_start) as u32);

        while head < level_end {
            let current = ws.queue[head];
            head += 1;
            farthest = current;

            for &next in g.neighbors(current) {
                if ws.visited_gen[next as usize] == gen {
                    continue; // すでに訪問済み
                }
                ws.queue.push(next);
                ws.parent[next as usize] = current;
                ws.visited_gen[next as usize] = gen;
            }
        }
        level_start = level_end; // 次のレベルの開始位置に更新
    }

    // 経路復元（index -> page_id 変換込み）
    let mut path = Vec::new();
    let mut node = farthest;
    while node != ROOT {
        path.push(page_ids[node as usize] as usize);
        node = ws.parent[node as usize];
    }
    path.reverse();

    (path, ws.distance_count.clone())
}
