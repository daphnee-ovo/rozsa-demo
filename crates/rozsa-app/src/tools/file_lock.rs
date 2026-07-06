use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

static FILE_LOCKS: Lazy<DashMap<PathBuf, Arc<Mutex<()>>>> = Lazy::new(DashMap::new);

pub async fn with_file_lock<F, T>(path: &PathBuf, f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let lock = FILE_LOCKS
        .entry(path.clone())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _guard = lock.lock().await;
    f.await
}
