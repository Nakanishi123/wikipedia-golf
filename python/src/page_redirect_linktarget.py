import argparse
import sqlite3
from pathlib import Path

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--page", type=Path, required=True, help="pagesのデータベースのパス")
    parser.add_argument("--redirect", type=Path, required=True, help="redirectのデータベースのパス")
    parser.add_argument("--linktarget", type=Path, required=True, help="linktargetのデータベースのパス")
    parser.add_argument("--output", "-o", type=Path, required=True, help="出力先のデータベースのパス")
    args = parser.parse_args()

    if not args.page.exists():
        print(f"{args.page}が存在しません")
        exit(1)
    if not args.redirect.exists():
        print(f"{args.redirect}が存在しません")
        exit(1)
    if not args.linktarget.exists():
        print(f"{args.linktarget}が存在しません")
        exit(1)

    conn = sqlite3.connect(f"{args.output}")
    cur = conn.cursor()
    cur.execute("ATTACH DATABASE ? AS page", (f"{args.page}",))
    cur.execute("ATTACH DATABASE ? AS redirect", (f"{args.redirect}",))
    cur.execute("ATTACH DATABASE ? AS linktarget", (f"{args.linktarget}",))
   
    cur.execute("CREATE TABLE page AS SELECT * FROM page.page WHERE page_namespace = 0")
    cur.execute("CREATE TABLE redirect AS SELECT * FROM redirect.redirect WHERE rd_namespace = 0 AND rd_interwiki = ''") # redirectをコピーしてかつ，rd_interwiki(他のウィキへのリンク)が空であるものを抽出(日本語版wikiのみ抽出する)
    cur.execute("CREATE TABLE linktarget AS SELECT * FROM linktarget.linktarget WHERE lt_namespace = 0")
    
    cur.execute("CREATE INDEX IF NOT EXISTS idx_page_title ON page (page_title)")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_rd_title ON redirect (rd_title)")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_lt_title ON linktarget (lt_title)")

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
    
    cur.execute("""
    CREATE TABLE linktarget_id AS
    SELECT
        lt_id AS lt_from_id,
        page_redirect.page_id AS lt_to_id
    FROM
        linktarget
    INNER JOIN
        page_redirect
    ON
        linktarget.lt_title = page_redirect.page_title
    ;
    """)
    
    # indexの作成
    cur.execute("CREATE INDEX IF NOT EXISTS idx_page_id ON page_redirect (page_id)")

    # page_redirect以外全部削除する
    cur.execute("DROP TABLE redirect")
    cur.execute("DROP TABLE page")
    cur.execute("DROP TABLE redirect_id")
    cur.execute("DROP TABLE linktarget")
    cur.execute("VACUUM")

    conn.close()
