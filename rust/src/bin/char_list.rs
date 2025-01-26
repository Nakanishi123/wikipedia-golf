use clap::Parser;
use std::{collections::HashMap, fs::write, path::PathBuf};
use wikipedia_golf::{structs::Page, util::load_bincode_zst};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, value_name = "PATH", help = "path to pages.bin.zst")]
    pages_bin: PathBuf,
    #[arg(long, short = 'o', value_name = "PATH", help = "output folder")]
    output_folder: PathBuf,
    #[arg(
        long,
        value_name = "u32",
        help = "minimum number of elements to separate into a file"
    )]
    separate: usize,
}

#[derive(Default, serde::Serialize)]
struct CharList {
    short: Vec<u32>,
    long: Vec<u32>,
}

fn main() {
    let args = Args::parse();
    let pages: Vec<Page> = load_bincode_zst(&args.pages_bin).unwrap();

    let char2page: HashMap<char, Vec<Page>> = pages
        .into_iter()
        .map(|page| (page.page_title.chars().next().unwrap(), page))
        .fold(HashMap::new(), |mut acc, (char, page)| {
            acc.entry(char).or_default().push(page);
            acc
        });

    let mut char2pages_short: Vec<Page> = char2page
        .clone()
        .into_iter()
        .filter(|(_, pages)| pages.len() < args.separate)
        .flat_map(|(_, pages)| pages)
        .collect();

    char2pages_short.sort_by(|a, b| a.page_title.cmp(&b.page_title));
    let output = serde_json::to_string(&char2pages_short).unwrap();
    write(args.output_folder.join("others.json"), output).unwrap();

    let char2pages_long: HashMap<char, Vec<Page>> = char2page
        .clone()
        .into_iter()
        .filter(|(_, pages)| pages.len() >= args.separate)
        .collect();

    for (char, pages) in &char2pages_long {
        let mut pages_cp = pages.clone();
        pages_cp.sort_by(|a, b| a.page_title.cmp(&b.page_title));

        let output = serde_json::to_string(&pages_cp).unwrap();
        let json_name = format!("{}.json", *char as u32);
        write(args.output_folder.join(json_name), output).unwrap();
    }

    let mut all_char: Vec<char> = char2page.keys().copied().collect();
    all_char.sort();
    let mut char_list = CharList::default();
    for &c in &all_char {
        if char2pages_long.contains_key(&c) {
            char_list.long.push(c as u32);
        } else {
            char_list.short.push(c as u32);
        }
    }

    let output = serde_json::to_string(&char_list).unwrap();
    write(args.output_folder.join("char_list.json"), output).unwrap();
}
