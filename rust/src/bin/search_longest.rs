use clap::Parser;
use num24::u24;
use rayon::prelude::*;
use serde_json::json;
use std::collections::{hash_map::Entry, HashMap, VecDeque};
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
static PAGE_EDGES: OnceLock<Arc<Vec<Vec<u24>>>> = OnceLock::new();

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let pages: Vec<Page> = load_bincode_zst(&args.pages_bin)?;
    let page_edges: Vec<Vec<u24>> = load_bincode_zst(&args.page_edges_bin)?;

    PAGES.set(Arc::new(pages)).unwrap();
    PAGE_EDGES.set(Arc::new(page_edges)).unwrap();

    let longest_paths = (0..PAGES.get().unwrap().len())
        .into_par_iter()
        .filter(|&id| id % args.matrix_size == args.matrix_num)
        .map(search_longest_path)
        .map(indices2id)
        .collect::<Vec<_>>();

    let longest_paths_json = longest_paths
        .into_iter()
        .map(|indeces| {
            json!({
                "id": indeces[0],
                "path": indeces,
            })
        })
        .collect::<Vec<_>>();

    let mut file = std::fs::File::create(&args.output_file)?;
    serde_json::to_writer(&mut file, &longest_paths_json)?;
    Ok(())
}

fn search_longest_path<T: Into<u24> + From<u24>>(start_index: T) -> Vec<T> {
    let mut queue: VecDeque<u24> = VecDeque::new();
    let mut parent: HashMap<u24, u24> = HashMap::new();

    let start_index_u24: u24 = start_index.into();
    queue.push_back(start_index_u24);
    parent.insert(start_index_u24, u24::MAX);

    let mut farthest_node = start_index_u24;
    while let Some(current) = queue.pop_front() {
        farthest_node = current;
        for &next in PAGE_EDGES.get().unwrap()[current.to_usize()].iter() {
            if let Entry::Vacant(e) = parent.entry(next) {
                e.insert(current);
                queue.push_back(next);
            }
        }
    }

    let mut longest_path = Vec::new();
    let mut node = farthest_node;
    while node != u24::MAX {
        longest_path.push(node.into());
        node = parent[&node];
    }

    longest_path.reverse();
    longest_path
}

fn indices2id<T: Into<usize>>(indices: Vec<T>) -> Vec<u32> {
    indices
        .into_iter()
        .map(|index| PAGES.get().unwrap()[index.into()].page_id)
        .collect()
}
