use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{path::Path, time::{SystemTime, UNIX_EPOCH}};

#[derive(Debug, Clone, Serialize)]
pub struct HistoryItem {
    pub id: i64, pub gid: String, pub url: String, pub referer: Option<String>,
    pub file_name: String, pub save_dir: String, pub category: String, pub status: String,
    pub total: u64, pub completed: u64, pub added_at: i64, pub finished_at: Option<i64>, pub error: Option<String>,
}

pub fn now() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64 }

pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let mut conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "foreign_keys", "ON").map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL").map_err(|e| e.to_string())?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> Result<(), String> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0)).map_err(|e| e.to_string())?;
    if version > 1 { return Err(format!("Database version {version} is newer than this app supports.")); }
    if version == 0 {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch("CREATE TABLE downloads (id INTEGER PRIMARY KEY, gid TEXT NOT NULL UNIQUE, url TEXT NOT NULL DEFAULT '', referer TEXT, file_name TEXT NOT NULL DEFAULT '', save_dir TEXT NOT NULL DEFAULT '', category TEXT NOT NULL DEFAULT 'Other', status TEXT NOT NULL DEFAULT 'waiting', total INTEGER NOT NULL DEFAULT 0, completed INTEGER NOT NULL DEFAULT 0, added_at INTEGER NOT NULL, finished_at INTEGER, error TEXT); CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL); CREATE TABLE categories (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, extensions TEXT NOT NULL, default_dir TEXT NOT NULL); PRAGMA user_version=1;").map_err(|e| e.to_string())?;
        let categories = [
            ("compressed", "Compressed", "zip,7z,rar,tar,gz,bz2,xz", "Compressed"),
            ("documents", "Documents", "pdf,doc,docx,odt,rtf,txt,md,xls,xlsx,ppt,pptx", "Documents"),
            ("music", "Music", "mp3,flac,wav,aac,ogg,m4a", "Music"),
            ("programs", "Programs", "exe,msi,msix,apk,deb,rpm,dmg,pkg,appimage", "Programs"),
            ("video", "Video", "mp4,mkv,avi,mov,webm,m4v,wmv", "Video"),
            ("other", "Other", "", "Other"),
        ];
        for (id, name, extensions, dir) in categories { tx.execute("INSERT INTO categories(id,name,extensions,default_dir) VALUES (?1,?2,?3,?4)", params![id,name,extensions,dir]).map_err(|e| e.to_string())?; }
        tx.execute("INSERT INTO settings(key,value) VALUES ('max_simultaneous_downloads','5'),('start_paused','false')", []).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn save_snapshot(conn: &Connection, gid: &str, url: &str, file_name: &str, save_dir: &str, category: &str, status: &str, total: u64, completed: u64, error: Option<&str>) -> Result<(), String> {
    let existing: Option<(i64, Option<i64>)> = conn.query_row("SELECT added_at, finished_at FROM downloads WHERE gid=?1", [gid], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(|e| e.to_string())?;
    let added_at = existing.map(|x| x.0).unwrap_or_else(now);
    let finished_at = if matches!(status, "complete" | "completed") { Some(now()) } else { None };
    conn.execute("INSERT INTO downloads(gid,url,file_name,save_dir,category,status,total,completed,added_at,finished_at,error) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(gid) DO UPDATE SET file_name=excluded.file_name,save_dir=excluded.save_dir,status=excluded.status,total=excluded.total,completed=excluded.completed,finished_at=COALESCE(downloads.finished_at,excluded.finished_at),error=excluded.error", params![gid,url,file_name,save_dir,category,status,total as i64,completed as i64,added_at,finished_at,error]).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn history(conn: &Connection) -> Result<Vec<HistoryItem>, String> {
    let mut stmt = conn.prepare("SELECT id,gid,url,referer,file_name,save_dir,category,status,total,completed,added_at,finished_at,error FROM downloads ORDER BY added_at DESC,id DESC").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok(HistoryItem { id:r.get(0)?,gid:r.get(1)?,url:r.get(2)?,referer:r.get(3)?,file_name:r.get(4)?,save_dir:r.get(5)?,category:r.get(6)?,status:r.get(7)?,total:r.get::<_,i64>(8)?.max(0) as u64,completed:r.get::<_,i64>(9)?.max(0) as u64,added_at:r.get(10)?,finished_at:r.get(11)?,error:r.get(12)? })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    if key.len() > 128 || value.len() > 4096 { return Err("Setting is too long.".into()); }
    conn.execute("INSERT INTO settings(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value]).map_err(|e| e.to_string())?; Ok(())
}
pub fn settings(conn: &Connection) -> Result<Vec<(String,String)>, String> {
    let mut stmt=conn.prepare("SELECT key,value FROM settings ORDER BY key").map_err(|e|e.to_string())?;
    stmt.query_map([],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[cfg(test)] mod tests {
 use super::*; use tempfile::tempdir;
 #[test] fn migrates_empty_database_and_persists_history() { let d=tempdir().unwrap(); let c=open(&d.path().join("app.db")).unwrap(); assert_eq!(c.pragma_query_value(None,"user_version",|r|r.get::<_,i64>(0)).unwrap(),1); save_snapshot(&c,"gid","https://example.test/a.zip","a.zip","/Downloads/Compressed","Compressed","complete",100,100,None).unwrap(); let h=history(&c).unwrap(); assert_eq!(h.len(),1); assert_eq!(h[0].file_name,"a.zip"); assert!(h[0].finished_at.is_some()); }
 #[test] fn upgrades_previous_version_one_without_resetting_history() { let d=tempdir().unwrap(); let p=d.path().join("app.db"); let c=Connection::open(&p).unwrap(); c.execute_batch("CREATE TABLE downloads (id INTEGER PRIMARY KEY,gid TEXT NOT NULL UNIQUE,url TEXT NOT NULL DEFAULT '',referer TEXT,file_name TEXT NOT NULL DEFAULT '',save_dir TEXT NOT NULL DEFAULT '',category TEXT NOT NULL DEFAULT 'Other',status TEXT NOT NULL DEFAULT 'waiting',total INTEGER NOT NULL DEFAULT 0,completed INTEGER NOT NULL DEFAULT 0,added_at INTEGER NOT NULL,finished_at INTEGER,error TEXT); CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE categories(id TEXT PRIMARY KEY,name TEXT NOT NULL UNIQUE,extensions TEXT NOT NULL,default_dir TEXT NOT NULL); PRAGMA user_version=1;").unwrap(); drop(c); let c=open(&p).unwrap(); assert!(history(&c).unwrap().is_empty()); }
 #[test] fn settings_round_trip() { let d=tempdir().unwrap(); let c=open(&d.path().join("app.db")).unwrap(); set_setting(&c,"theme","dark").unwrap(); assert!(settings(&c).unwrap().contains(&("theme".into(),"dark".into()))); }
}
