pub mod migrations;
pub mod queries;

use anyhow::Result;
use deadpool_sqlite::{Config, Pool, Runtime};

pub fn create_pool(db_path: &str) -> Result<Pool> {
    // Register sqlite-vec auto extension before creating any connections
    unsafe {
        use rusqlite::ffi::sqlite3_auto_extension;
        sqlite3_auto_extension(Some(std::mem::transmute::<
            *const (),
            unsafe extern "C" fn(
                *mut rusqlite::ffi::sqlite3,
                *mut *mut i8,
                *const rusqlite::ffi::sqlite3_api_routines,
            ) -> i32,
        >(sqlite_vec::sqlite3_vec_init as *const ())));
    }

    let cfg = Config::new(db_path);
    let pool = cfg.builder(Runtime::Tokio1)?.build()?;
    Ok(pool)
}

pub fn setup_connection(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    Ok(())
}

pub fn open_and_migrate(db_path: &str) -> Result<rusqlite::Connection> {
    // Register sqlite-vec auto extension before opening connection
    unsafe {
        use rusqlite::ffi::sqlite3_auto_extension;
        sqlite3_auto_extension(Some(std::mem::transmute::<
            *const (),
            unsafe extern "C" fn(
                *mut rusqlite::ffi::sqlite3,
                *mut *mut i8,
                *const rusqlite::ffi::sqlite3_api_routines,
            ) -> i32,
        >(sqlite_vec::sqlite3_vec_init as *const ())));
    }

    let mut conn = rusqlite::Connection::open(db_path)?;
    setup_connection(&conn)?;
    migrations::run(&mut conn)?;
    Ok(conn)
}
