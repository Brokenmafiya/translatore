use rusqlite::{params, Connection, Result};

use super::Rule;

pub fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS rules (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pattern TEXT NOT NULL,
            rule_type TEXT NOT NULL DEFAULT 'http',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )?;
    Ok(())
}

pub fn add_rule(conn: &Connection, pattern: &str, rule_type: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO rules (pattern, rule_type) VALUES (?1, ?2)",
        params![pattern, rule_type],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_rules(conn: &Connection) -> Vec<Rule> {
    let mut stmt = conn
        .prepare("SELECT pattern, rule_type FROM rules ORDER BY id")
        .unwrap();
    stmt.query_map([], |row| {
        Ok(Rule {
            pattern: row.get(0)?,
            rule_type: row.get(1)?,
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

pub fn delete_rule(conn: &Connection, id: i64) -> Result<()> {
    let changed = conn.execute("DELETE FROM rules WHERE id = ?1", params![id])?;
    if changed == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}
