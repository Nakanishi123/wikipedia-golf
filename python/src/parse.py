import argparse
import sqlite3
from pathlib import Path

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--page", type=Path, required=True, help="pagesのデータベースのパス")
    parser.add_argument("--redirect", type=Path, required=True, help="redirectのデータベースのパス")
    parser.add_argument("--linktarget", type=Path, required=True, help="linktargetのデータベースのパス")
    parser.add_argument("--pagelinks", type=Path, required=True, help="pagelinksのデータベースのパス")
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
    if not args.pagelinks.exists():
        print(f"{args.pagelinks}が存在しません")
        exit(1)
        

    conn = sqlite3.connect(f"{args.output}")
    cur = conn.cursor()
    
    # データベースをアタッチ
    cur.execute("ATTACH DATABASE ? AS page", (f"{args.page}",))
    cur.execute("ATTACH DATABASE ? AS redirect", (f"{args.redirect}",))
    cur.execute("ATTACH DATABASE ? AS linktarget", (f"{args.linktarget}",))
    cur.execute("ATTACH DATABASE ? AS pagelinks", (f"{args.pagelinks}",))
    
    # テーブルをコピー
    cur.execute("CREATE TABLE page AS SELECT * FROM page.page WHERE page_namespace = 0")
    cur.execute("CREATE TABLE redirect AS SELECT * FROM redirect.redirect WHERE rd_namespace = 0 AND rd_interwiki = ''") # redirectをコピーしてかつ，rd_interwiki(他のウィキへのリンク)が空であるものを抽出(日本語版wikiのみ抽出する)
    cur.execute("CREATE TABLE linktarget AS SELECT * FROM linktarget.linktarget WHERE lt_namespace = 0")
    cur.execute("CREATE TABLE pagelinks AS SELECT * FROM pagelinks.pagelinks WHERE pl_from_namespace = 0")
    
    # データベースのデタッチ
    cur.execute("DETACH DATABASE page")
    cur.execute("DETACH DATABASE redirect")
    cur.execute("DETACH DATABASE linktarget")
    cur.execute("DETACH DATABASE pagelinks")
    
    # コピーしたデータベースを削除
    args.page.unlink()
    args.redirect.unlink()
    args.linktarget.unlink()
    args.pagelinks.unlink()
    
    # indexの作成
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
    cur.execute("CREATE INDEX IF NOT EXISTS idx_page_id ON page_redirect (page_id)")
    
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
    
    cur.execute("""
    CREATE TABLE pagelinks_id AS
    SELECT
        pl_from AS pl_from_id,
        linktarget_id.lt_to_id AS pl_to_id
    FROM
        pagelinks
    INNER JOIN
        linktarget_id
    ON
        pagelinks.pl_target_id = linktarget_id.lt_from_id
    INNER JOIN
        page_redirect par_from
    ON
        pagelinks.pl_from = par_from.page_id
    INNER JOIN
        page_redirect par_to
    ON
        linktarget_id.lt_to_id = par_to.page_id
    ;
    """)
    cur.execute("CREATE INDEX IF NOT EXISTS idx_pl_to_id ON pagelinks_id (pl_to_id)")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_pl_from_id ON pagelinks_id (pl_from_id)")

    # page_redirect以外全部削除する
    cur.execute("DROP TABLE redirect")
    cur.execute("DROP TABLE page")
    cur.execute("DROP TABLE redirect_id")
    cur.execute("DROP TABLE linktarget")
    cur.execute("DROP TABLE linktarget_id")
    cur.execute("DROP TABLE pagelinks")
    cur.execute("VACUUM")

    conn.close()
