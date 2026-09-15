//! Payload-free request timing. The guard also removes canceled futures.
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Default)]
pub(crate) struct Requests {
    next: AtomicU64,
    active: Mutex<BTreeMap<u64, (&'static str, Instant)>>,
}
pub(crate) struct Request {
    owner: Arc<Requests>,
    id: u64,
}
impl Requests {
    pub fn start(self: &Arc<Self>, method: &'static str) -> Request {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.active
            .lock()
            .unwrap()
            .insert(id, (method, Instant::now()));
        Request {
            owner: self.clone(),
            id,
        }
    }
    pub fn slow(&self, threshold: Duration) -> Vec<String> {
        self.active
            .lock()
            .unwrap()
            .values()
            .filter(|(_, start)| start.elapsed() >= threshold)
            .map(|(method, _)| method.to_string())
            .collect()
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        self.owner.active.lock().unwrap().remove(&self.id);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canceled_requests_are_removed_and_parallel_calls_remain_distinct() {
        let requests = Arc::new(Requests::default());
        let a = requests.start("test.call");
        let b = requests.start("test.call");
        assert_eq!(requests.slow(Duration::ZERO).len(), 2);
        assert!(requests.slow(Duration::from_secs(60)).is_empty());
        drop(a);
        assert_eq!(requests.slow(Duration::ZERO), vec!["test.call"]);
        drop(b);
        assert!(requests.slow(Duration::ZERO).is_empty());
    }
}
