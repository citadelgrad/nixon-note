use axum::Router;
use deadpool_sqlite::{Config, Pool, Runtime};
use note::{AppState, background, db};

pub async fn setup_test_app() -> Router {
    // Register sqlite-vec extension globally for tests
    unsafe {
        use rusqlite::ffi::sqlite3_auto_extension;
        sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    }

    // Create in-memory SQLite database for testing
    let pool = create_test_pool().await;

    // Run migrations
    let conn = pool.get().await.unwrap();
    conn.interact(|conn: &mut rusqlite::Connection| db::migrations::run(conn))
        .await
        .unwrap()
        .unwrap();

    // Create HTTP client
    let client = reqwest::Client::new();

    // Create background processor
    let bg = background::BackgroundProcessor::new(pool.clone(), client.clone());

    // Create app state
    let state = AppState {
        pool,
        background: bg,
        client,
    };

    // Set test environment variables
    unsafe {
        std::env::set_var("NOTE_TOKEN", "test-token");
    }

    // Build the router
    note::create_router(state)
}

async fn create_test_pool() -> Pool {
    let config = Config::new(":memory:");
    config.create_pool(Runtime::Tokio1).unwrap()
}
