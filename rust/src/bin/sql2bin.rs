use clap::Parser;
use num24::u24;
use rusqlite::{Connection, Result};
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use wikipedia_golf::structs::{Page, VarintBiCsr, VarintCsr};
use wikipedia_golf::util::save_bincode_zst;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, value_name = "PATH", help = "path to db file")]
    db_path: PathBuf,
    #[arg(long, short = 'o', value_name = "PATH", help = "output folder")]
    output_folder: PathBuf,
}

fn encode_varint_csr(adj: &[Vec<u32>]) -> Result<VarintCsr, Box<dyn Error>> {
    let n = adj.len();
    let mut offsets = Vec::with_capacity(n + 1);
    let mut data = Vec::new();
    let mut tmp = Vec::new();

    offsets.push(0u32);
    for list in adj {
        tmp.clear();
        tmp.extend_from_slice(list);
        tmp.sort_unstable();
        tmp.dedup();
        let mut prev = 0u32;
        for &v in &tmp {
            let mut d = v - prev;
            prev = v;
            while d >= 0x80 {
                data.push((d as u8) | 0x80);
                d >>= 7;
            }
            data.push(d as u8);
        }
        let offset = u32::try_from(data.len())
            .map_err(|_| format!("VarintCsr byte length ({}) exceeds u32::MAX", data.len()))?;
        offsets.push(offset);
    }
    data.shrink_to_fit();
    Ok(VarintCsr { offsets, data })
}

fn build_vcs_graph(
    pages: &[Page],
    page_edges: &[Vec<u24>],
) -> Result<(Vec<Page>, VarintBiCsr), Box<dyn Error>> {
    let n = pages.len();

    // Compute in-degrees to seed BFS from highest in-degree hubs
    let mut in_deg = vec![0u32; n];
    for list in page_edges {
        for &v24 in list {
            let v: u32 = v24.into();
            in_deg[v as usize] += 1;
        }
    }
    let mut seeds: Vec<u32> = (0..n as u32).collect();
    seeds.sort_unstable_by(|&a, &b| in_deg[b as usize].cmp(&in_deg[a as usize]));

    // BFS ordering improves locality for both forward and reverse edges
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let mut queue = Vec::with_capacity(n);
    for &start in &seeds {
        if visited[start as usize] {
            continue;
        }
        visited[start as usize] = true;
        queue.clear();
        queue.push(start);
        let mut head = 0;
        while head < queue.len() {
            let u = queue[head];
            head += 1;
            order.push(u);
            for &v24 in &page_edges[u as usize] {
                let v: u32 = v24.into();
                if !visited[v as usize] {
                    visited[v as usize] = true;
                    queue.push(v);
                }
            }
        }
    }

    let mut old_to_new = vec![0u32; n];
    for (new_id, &old_id) in order.iter().enumerate() {
        old_to_new[old_id as usize] = new_id as u32;
    }

    let vcs_pages: Vec<Page> = order
        .iter()
        .map(|&old_id| pages[old_id as usize].clone())
        .collect();

    let mut fwd_adj = vec![Vec::new(); n];
    let mut rev_adj = vec![Vec::new(); n];
    for (old_u, list) in page_edges.iter().enumerate() {
        let nu = old_to_new[old_u];
        fwd_adj[nu as usize].reserve(list.len());
        for &v24 in list {
            let old_v: u32 = v24.into();
            let nv = old_to_new[old_v as usize];
            fwd_adj[nu as usize].push(nv);
            rev_adj[nv as usize].push(nu);
        }
    }

    let fwd = encode_varint_csr(&fwd_adj)?;
    drop(fwd_adj);
    let rev = encode_varint_csr(&rev_adj)?;
    drop(rev_adj);

    Ok((vcs_pages, VarintBiCsr { fwd, rev }))
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
            page_redirect_id: row.get::<_, u32>(3).ok(),
        })
    })?;

    let mut pages: Vec<Page> = Vec::new();
    for page in pages_iter {
        pages.push(page?);
    }

    // page_titleでソート
    pages.sort_by(|a, b| a.page_title.cmp(&b.page_title));

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

    // 既存形式をバイナリ形式で保存（互換性維持）
    save_bincode_zst(args.output_folder.join("pages.bin.zst"), &pages)?;
    save_bincode_zst(args.output_folder.join("page_edges.bin.zst"), &page_edges)?;

    // BFS順・双方向 Delta+Varint CSR 形式を保存（省メモリサーバー向け）
    let (vcs_pages, vcs_graph) = build_vcs_graph(&pages, &page_edges)?;
    save_bincode_zst(args.output_folder.join("vcs_pages.bin.zst"), &vcs_pages)?;
    save_bincode_zst(args.output_folder.join("vcs_page_edges.bin.zst"), &vcs_graph)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_neighbors(csr: &VarintCsr, u: usize) -> Vec<u32> {
        let mut pos = csr.offsets[u] as usize;
        let end = csr.offsets[u + 1] as usize;
        let mut out = Vec::new();
        let mut prev = 0u32;
        while pos < end {
            let mut delta = 0u32;
            let mut shift = 0u32;
            loop {
                let b = csr.data[pos];
                pos += 1;
                delta |= ((b & 0x7F) as u32) << shift;
                if (b & 0x80) == 0 {
                    break;
                }
                shift += 7;
            }
            prev += delta;
            out.push(prev);
        }
        out
    }

    #[test]
    fn test_build_vcs_graph_roundtrip() {
        let pages: Vec<Page> = (0..5u32)
            .map(|i| Page {
                page_id: 100 + i,
                page_title: format!("p{}", i),
                page_is_redirect: 0,
                page_redirect_id: None,
            })
            .collect();
        let raw_edges: Vec<Vec<u32>> = vec![
            vec![1, 2],
            vec![2, 3, 2], // duplicate edge should be deduplicated
            vec![0],
            vec![4],
            vec![],
        ];
        let page_edges: Vec<Vec<u24>> = raw_edges
            .iter()
            .map(|v| v.iter().map(|&x| x.into()).collect())
            .collect();

        let (vcs_pages, vcs_graph) = build_vcs_graph(&pages, &page_edges).unwrap();
        assert_eq!(vcs_pages.len(), pages.len());

        let mut fwd_pairs: Vec<(u32, u32)> = Vec::new();
        let mut rev_pairs: Vec<(u32, u32)> = Vec::new();
        for u in 0..vcs_pages.len() {
            for v in decode_neighbors(&vcs_graph.fwd, u) {
                fwd_pairs.push((vcs_pages[u].page_id, vcs_pages[v as usize].page_id));
            }
            for src in decode_neighbors(&vcs_graph.rev, u) {
                rev_pairs.push((vcs_pages[src as usize].page_id, vcs_pages[u].page_id));
            }
        }
        fwd_pairs.sort();
        rev_pairs.sort();
        let expected = vec![(100, 101), (100, 102), (101, 102), (101, 103), (102, 100), (103, 104)];
        assert_eq!(fwd_pairs, expected);
        assert_eq!(rev_pairs, expected);
    }
}
