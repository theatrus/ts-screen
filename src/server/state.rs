use anyhow::Result;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub db_path: String,
    pub image_dir: PathBuf,
    pub cache_dir: PathBuf,
    // We'll use a connection pool or create connections as needed
    db_connection: Arc<Mutex<Connection>>,
}

impl AppState {
    pub fn new(db_path: String, image_dir: String, cache_dir: String) -> Result<Self> {
        use std::path::Path;
        
        // Check if database exists
        if !Path::new(&db_path).exists() {
            return Err(anyhow::anyhow!("Database file not found: {}", db_path));
        }
        
        // Check if image directory exists
        if !Path::new(&image_dir).exists() {
            return Err(anyhow::anyhow!("Image directory not found: {}", image_dir));
        }
        
        // Open database connection
        let conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        Ok(Self {
            db_path,
            image_dir: PathBuf::from(image_dir),
            cache_dir: PathBuf::from(cache_dir),
            db_connection: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn db(&self) -> Arc<Mutex<Connection>> {
        self.db_connection.clone()
    }

    pub fn get_cache_path(&self, category: &str, filename: &str) -> PathBuf {
        self.cache_dir.join(category).join(filename)
    }

    pub fn get_image_path(&self, relative_path: &str) -> PathBuf {
        self.image_dir.join(relative_path)
    }
}
