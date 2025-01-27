use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{fs::write, path::PathBuf};
use wikipedia_golf::{structs::Page, util::load_bincode_zst};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, value_name = "PATH", help = "path to pages.bin.zst")]
    pages_bin: PathBuf,
    #[arg(long, short = 'o', value_name = "PATH", help = "output folder")]
    output_folder: PathBuf,
    #[arg(long, value_name = "usize", help = "Number of files to generate")]
    file_num: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageAndRedirect {
    pub page_id: u32,
    pub page_title: String,
    pub page_is_redirect: u32,
    pub page_redirect_id: Option<u32>,
    pub redirect_title: Option<String>,
}

fn main() {
    let args = Args::parse();
    let pages: Vec<Page> = load_bincode_zst(args.pages_bin).unwrap();

    let id2title: std::collections::HashMap<u32, String> = pages
        .iter()
        .map(|p| (p.page_id, p.page_title.clone()))
        .collect();

    let pages: Vec<PageAndRedirect> = pages
        .into_iter()
        .map(|p| PageAndRedirect {
            page_id: p.page_id,
            page_title: p.page_title,
            page_is_redirect: p.page_is_redirect,
            page_redirect_id: p.page_redirect_id,
            redirect_title: p.page_redirect_id.map(|id| id2title[&id].clone()),
        })
        .collect();

    let num_par_file = (pages.len() / args.file_num) + 1;
    let mut title_list = vec![];
    for (i, p) in chunk_with_overlap(pages, num_par_file, 15).enumerate() {
        title_list.push(p[0].page_title.clone());
        let output = serde_json::to_string(&p).unwrap();
        write(args.output_folder.join(format!("{}.json", i)), output).unwrap();
    }
    let output = serde_json::to_string(&title_list).unwrap();
    write(args.output_folder.join("title_list.json"), output).unwrap();
}

fn chunk_with_overlap<I: IntoIterator<Item = T>, T: Clone>(
    iter: I,
    chunk_size: usize,
    overlap: usize,
) -> impl Iterator<Item = Vec<I::Item>> {
    let mut iter = iter.into_iter();
    let mut buffer: Vec<I::Item> = Vec::new();
    std::iter::from_fn(move || {
        while buffer.len() < chunk_size + overlap {
            if let Some(item) = iter.next() {
                buffer.push(item);
            } else {
                break;
            }
        }
        if buffer.is_empty() {
            None
        } else {
            let chunk = buffer.clone();
            buffer.drain(..chunk_size.min(buffer.len()));
            Some(chunk)
        }
    })
}
