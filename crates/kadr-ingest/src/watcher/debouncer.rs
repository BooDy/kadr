use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct DebounceQueue {
    quiet_duration: Duration,
    pending: HashMap<PathBuf, Instant>,
}

impl DebounceQueue {
    pub fn new(quiet_duration: Duration) -> Self {
        Self {
            quiet_duration,
            pending: HashMap::new(),
        }
    }

    pub fn record_event(&mut self, path: PathBuf) {
        self.pending.insert(path, Instant::now());
    }

    pub fn remove(&mut self, path: &Path) {
        self.pending.remove(path);
    }

    pub fn extract_settled(&mut self) -> Vec<PathBuf> {
        let now = Instant::now();
        let mut settled = Vec::new();

        self.pending.retain(|path, last_seen| {
            if now.checked_duration_since(*last_seen).unwrap_or_default() >= self.quiet_duration {
                settled.push(path.clone());
                false
            } else {
                true
            }
        });

        settled
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debounce_queue_settles_after_duration() {
        let mut queue = DebounceQueue::new(Duration::from_millis(50));
        let path1 = PathBuf::from("/test/movie1.mkv");
        let path2 = PathBuf::from("/test/movie2.mkv");

        queue.record_event(path1.clone());
        assert_eq!(queue.len(), 1);

        // Immediate check: nothing settled
        let settled = queue.extract_settled();
        assert!(settled.is_empty());
        assert_eq!(queue.len(), 1);

        std::thread::sleep(Duration::from_millis(60));

        // Now path1 should be settled
        let settled = queue.extract_settled();
        assert_eq!(settled, vec![path1.clone()]);
        assert_eq!(queue.len(), 0);

        // Recording again and removing before settling
        queue.record_event(path2.clone());
        assert_eq!(queue.len(), 1);
        queue.remove(&path2);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_debounce_queue_sliding_window() {
        let mut queue = DebounceQueue::new(Duration::from_millis(80));
        let path = PathBuf::from("/test/movie.mkv");

        queue.record_event(path.clone());
        std::thread::sleep(Duration::from_millis(50));

        // Record again to slide window
        queue.record_event(path.clone());
        std::thread::sleep(Duration::from_millis(50));

        // 100ms total has passed, but only 50ms since last event: not settled
        let settled = queue.extract_settled();
        assert!(settled.is_empty());

        // Wait another 40ms (90ms since last event)
        std::thread::sleep(Duration::from_millis(40));
        let settled = queue.extract_settled();
        assert_eq!(settled, vec![path]);
    }
}
