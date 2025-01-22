import argparse
import sqlite3
from pathlib import Path

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--page", type=Path, required=True, help="pagesのデータベースのパス")
    parser.add_argument("--redirect", type=Path, required=True, help="redirectのデータベースのパス")
    parser.add_argument("--output", "-o", type=Path, required=True, help="出力先のデータベースのパス")
    args = parser.parse_args()

    conn = sqlite3.connect(f"{args.output}")
    cur = conn.cursor()
    cur.execute("ATTACH DATABASE ? AS redirect", (f"{args.redirect}",))
    cur.execute("ATTACH DATABASE ? AS page", (f"{args.page}",))
    # redirectをコピーしてかつ，rd_interwiki(他のウィキへのリンク)が空であるものを抽出(日本語版wikiのみ抽出する)
    cur.execute("CREATE TABLE redirect AS SELECT * FROM redirect.redirect WHERE rd_namespace = 0 AND rd_interwiki = ''")
    cur.execute("CREATE TABLE page AS SELECT * FROM page.page WHERE page_namespace = 0")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_page_title ON page (page_title)")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_rd_title ON redirect (rd_title)")

    cur.execute("""
    CREATE TABLE redirect_id AS
    SELECT
        rd_from AS rd_from_id,
        page.page_id AS rd_to_id
    FROM
        redirect
    INNER JOIN
        page
    ON
        rd_title = page.page_title
    ;
    """)
    cur.execute("""
    CREATE TABLE page_redirect AS
    SELECT
        page.page_id,
        page.page_title,
        page.page_is_redirect,
        rd_to_id as page_redirect_id
    FROM
        page
    LEFT JOIN
        redirect_id
    ON
        page.page_id = redirect_id.rd_from_id
    WHERE
        page.page_is_redirect =0 
    OR
        page.page_is_redirect =1 AND page_redirect_id IS NOT NULL
    ;
    """)

    # page_redirect以外全部削除する
    cur.execute("DROP TABLE redirect")
    cur.execute("DROP TABLE page")
    cur.execute("DROP TABLE redirect_id")
    cur.execute("VACUUM")

    conn.close()
