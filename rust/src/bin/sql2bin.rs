use clap::Parser;
use num24::u24;
use rusqlite::{Connection, Result};
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use wikipedia_golf::structs::Page;
use wikipedia_golf::util::save_bincode_zst;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, value_name = "PATH", help = "path to db file")]
    db_path: PathBuf,
    #[arg(long, short = 'o', value_name = "PATH", help = "output folder")]
    output_folder: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let conn = Connection::open(args.db_path)?;
    // `page_redirect` テーブルのデータを読み取る
    let mut stmt = conn.prepare(
        "SELECT page_id, page_title, page_is_redirect, page_redirect_id FROM page_redirect",
    )?;
    let pages_iter = stmt.query_map([], |row| {
        Ok(Page {
            page_id: (row.get::<_, u32>(0)?),
            page_title: row.get(1)?,
            page_is_redirect: (row.get::<_, u32>(2)?),
            page_redirect_id: match row.get::<_, u32>(3) {
                Ok(id) => Some(id),
                Err(_) => None,
            },
        })
    })?;

    let mut pages: Vec<Page> = Vec::new();
    for page in pages_iter {
        pages.push(page?);
    }

    // ページ ID からインデックスへのマッピングを作成
    let mut page2index: HashMap<u24, usize> = HashMap::with_capacity(pages.len());
    for (i, page) in pages.iter().enumerate() {
        page2index.insert(page.page_id.into(), i);
    }

    // `pagelinks_id` テーブルのデータを読み取る
    let mut stmt = conn.prepare("SELECT pl_from_id, pl_to_id FROM pagelinks_id")?;
    let links_iter =
        stmt.query_map([], |row| Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?)))?;

    // edge のリストを作成(index に変換している)
    let mut page_edges: Vec<Vec<u24>> = vec![Vec::new(); pages.len()];
    for link in links_iter {
        let link = link?;
        let from = page2index.get(&link.0.into()).unwrap();
        let to = page2index.get(&link.1.into()).unwrap();
        page_edges[*from].push((*to).into());
    }

    // データをバイナリ形式で保存
    save_bincode_zst(args.output_folder.join("pages.bin.zst"), &pages)?;
    save_bincode_zst(args.output_folder.join("page_edges.bin.zst"), &page_edges)?;

    Ok(())
}
