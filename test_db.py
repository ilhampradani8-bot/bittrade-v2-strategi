import psycopg2
try:
    conn = psycopg2.connect(
        database="bottrade_db",
        user="bottrade_user",
        password="@Dani22334455D",
        host="localhost",
        port="5432"
    )
    cur = conn.cursor()
    cur.execute("SELECT * FROM alt_balance_history ORDER BY id DESC LIMIT 5;")
    rows = cur.fetchall()
    print("ROWS IN alt_balance_history:")
    for r in rows:
        print(r)
    cur.close()
    conn.close()
except Exception as e:
    print("ERROR:", e)
