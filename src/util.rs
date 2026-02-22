use std::{
    fs,
    future::Future,
    path::Path,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context as AnyhowContext;
use rand::random;

pub fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn new_session_id() -> String {
    format!("session-{}-{:08x}", unix_timestamp_secs(), random::<u32>())
}

pub fn ensure_parent_dir_exists(path: &Path) -> anyhow::Result<()> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory {}", parent.display())),
        _ => Ok(()),
    }
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

pub fn block_on_future<F>(future: F) -> F::Output
where
    F: Future,
{
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);

    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
