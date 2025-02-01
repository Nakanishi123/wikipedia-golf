use clap::Parser;
use num24::u24;
use rayon::prelude::*;
use serde_json::json;
use std::collections::VecDeque;
use std::error::Error;
use std::sync::{Arc, OnceLock};
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

static PAGES: OnceLock<Arc<Vec<Page>>> = OnceLock::new();
static PAGE_EDGES: OnceLock<Arc<Vec<Vec<u32>>>> = OnceLock::new();

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let pages: Vec<Page> = load_bincode_zst(&args.pages_bin)?;
    let page_edges: Vec<Vec<u24>> = load_bincode_zst(&args.page_edges_bin)?;

    // u32の方が速いので変換
    let page_edges = page_edges
        .into_iter()
        .map(|v| v.into_iter().map(|x| x.into()).collect())
        .collect();

    PAGES.set(Arc::new(pages)).unwrap();
    PAGE_EDGES.set(Arc::new(page_edges)).unwrap();

    let longest_paths = (0..PAGES.get().unwrap().len())
        .into_par_iter()
        .filter(|&id| id % args.matrix_size == args.matrix_num)
        .map(|id: usize| search_longest_path(id as u32))
        .map(|result| (indices2id(result.0), result.1))
        .collect::<Vec<_>>();

    let longest_paths_json = longest_paths
        .into_iter()
        .map(|indeces| {
            json!({
                "id": indeces.0[0],
                "path": indeces.0,
                "distance_count": indeces.1,
            })
        })
        .collect::<Vec<_>>();

    let mut file = std::fs::File::create(&args.output_file)?;
    serde_json::to_writer(&mut file, &longest_paths_json)?;
    Ok(())
}

fn search_longest_path(start_index: u32) -> (Vec<usize>, Vec<u32>) {
    let mut queue: VecDeque<u32> = VecDeque::new();
    let mut parent: Vec<u32> = vec![u32::MAX; PAGES.get().unwrap().len()];

    let root = u32::MAX - 1;
    let not_visited = u32::MAX;
    queue.push_back(start_index);
    parent[start_index as usize] = root;

    let mut distance_count: Vec<u32> = vec![1; 1];
    let mut distance_now = 0;
    let mut distance_change = start_index;
    let mut distance_change_now = false;

    let mut farthest_node = start_index;
    while let Some(current) = queue.pop_front() {
        if current == distance_change {
            distance_now += 1;
            distance_change_now = true;
        }
        farthest_node = current;
        for &next in PAGE_EDGES.get().unwrap()[current as usize].iter() {
            if parent[next as usize] == not_visited {
                if distance_change_now {
                    distance_count.push(1);
                    distance_change = next;
                    distance_change_now = false;
                } else {
                    distance_count[distance_now] += 1;
                }
                parent[next as usize] = current;
                queue.push_back(next);
            }
        }
    }

    let mut longest_path = Vec::new();
    let mut node = farthest_node;
    while node != root {
        longest_path.push(node as usize);
        node = parent[node as usize];
    }

    longest_path.reverse();
    (longest_path, distance_count)
}

fn indices2id<T: Into<usize>>(indices: Vec<T>) -> Vec<u32> {
    indices
        .into_iter()
        .map(|index| PAGES.get().unwrap()[index.into()].page_id)
        .collect()
}
