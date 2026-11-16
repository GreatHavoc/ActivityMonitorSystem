//! Priority queue + worker manager — port of RequestQueueManager and
//! PriorityBlockingCollection (one unbounded channel per priority level,
//! manual capacity counter, worker semaphore, drop-when-full).
//! Fix: the C# capacity check was increment-then-compare (the count could
//! transiently exceed max under races); the port rejects atomically with a
//! CAS loop.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jiff::{Span, Timestamp};
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinHandle;

use crate::capture::capture_frames;
use crate::config::{ActivityMonitorSettings, CaptureSettings};
use crate::db::ActivityDatabase;
use crate::models::{InferenceRequest, QueueMetrics, RequestPriority};
use crate::ollama::{wait_cancel, OllamaClient};

const MAX_PROCESSING_TIME_SAMPLES: usize = 100;
const PRIORITY_LEVELS: usize = 4; // Low..Critical

/// Port of PriorityBlockingCollection<InferenceRequest>.
struct PriorityQueue {
    senders: Vec<mpsc::UnboundedSender<InferenceRequest>>,
    receivers: Mutex<Vec<mpsc::UnboundedReceiver<InferenceRequest>>>,
    available: Arc<Semaphore>,
    count: AtomicI64,
    max: i64,
}

impl PriorityQueue {
    fn new(max: i64) -> Self {
        let mut senders = Vec::with_capacity(PRIORITY_LEVELS);
        let mut receivers = Vec::with_capacity(PRIORITY_LEVELS);
        for _ in 0..PRIORITY_LEVELS {
            let (tx, rx) = mpsc::unbounded_channel();
            senders.push(tx);
            receivers.push(rx);
        }
        Self {
            senders,
            receivers: Mutex::new(receivers),
            available: Arc::new(Semaphore::new(0)),
            count: AtomicI64::new(0),
            max,
        }
    }

    fn try_add(&self, item: InferenceRequest, priority: RequestPriority) -> bool {
        let mut cur = self.count.load(Ordering::Relaxed);
        loop {
            if cur >= self.max {
                return false;
            }
            match self.count.compare_exchange_weak(cur, cur + 1, Ordering::AcqRel, Ordering::Relaxed) {
                Ok(_) => break,
                Err(c) => cur = c,
            }
        }
        if self.senders[priority as usize].send(item).is_err() {
            self.count.fetch_sub(1, Ordering::Relaxed);
            return false;
        }
        self.available.add_permits(1);
        true
    }

    /// Highest priority first, FIFO within a level. None when cancelled.
    async fn take(&self, cancel: &AtomicBool) -> Option<InferenceRequest> {
        loop {
            let permit = tokio::select! {
                _ = wait_cancel(cancel) => return None,
                p = self.available.acquire() => p.ok()?,
            };
            drop(permit);
            let mut receivers = self.receivers.lock().unwrap();
            for rx in receivers.iter_mut().rev() {
                if let Ok(item) = rx.try_recv() {
                    self.count.fetch_sub(1, Ordering::Relaxed);
                    return Some(item);
                }
            }
            // Spurious wakeup (another taker won the race): wait again, as in C#.
        }
    }

    fn count(&self) -> i64 {
        self.count.load(Ordering::Relaxed)
    }
}

pub struct RequestQueueManager {
    queue: PriorityQueue,
    workers: Arc<Semaphore>,
    capture_cfg: CaptureSettings,
    ollama: Arc<OllamaClient>,
    db: Arc<Mutex<ActivityDatabase>>,
    cancel: Arc<AtomicBool>,
    loop_handle: Mutex<Option<JoinHandle<()>>>,
    active: Mutex<Vec<JoinHandle<()>>>,
    active_count: AtomicI64,
    total_processed: AtomicI64,
    total_failed: AtomicI64,
    times: Mutex<VecDeque<Duration>>,
}

impl RequestQueueManager {
    pub fn new(
        settings: &ActivityMonitorSettings,
        ollama: Arc<OllamaClient>,
        db: Arc<Mutex<ActivityDatabase>>,
    ) -> Self {
        let q = &settings.queue_settings;
        Self {
            queue: PriorityQueue::new(q.max_queue_size),
            workers: Arc::new(Semaphore::new(q.max_concurrent_tasks.max(1) as usize)),
            capture_cfg: settings.capture_settings.clone(),
            ollama,
            db,
            cancel: Arc::new(AtomicBool::new(false)),
            loop_handle: Mutex::new(None),
            active: Mutex::new(Vec::new()),
            active_count: AtomicI64::new(0),
            total_processed: AtomicI64::new(0),
            total_failed: AtomicI64::new(0),
            times: Mutex::new(VecDeque::new()),
        }
    }

    pub fn start(self: &Arc<Self>) {
        let mut h = self.loop_handle.lock().unwrap();
        if h.is_some() {
            return;
        }
        tracing::info!(
            workers = self.workers.available_permits(),
            "Starting Request Queue Manager"
        );
        *h = Some(tokio::spawn(Arc::clone(self).run()));
    }

    pub async fn stop(&self) {
        tracing::info!("Stopping Request Queue Manager");
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(h) = self.loop_handle.lock().unwrap().take() {
            let _ = h.await;
        }
        let handles: Vec<JoinHandle<()>> = std::mem::take(&mut *self.active.lock().unwrap());
        for h in handles {
            let _ = h.await; // graceful drain, as in C# Task.WhenAll(_activeTasks)
        }
        tracing::info!(
            processed = self.total_processed.load(Ordering::Relaxed),
            failed = self.total_failed.load(Ordering::Relaxed),
            "Request Queue Manager stopped"
        );
    }

    /// Returns false when the queue is full (drop-when-full, as in C#).
    pub fn enqueue(&self, request: InferenceRequest) -> bool {
        let priority = request.priority;
        if self.queue.try_add(request, priority) {
            tracing::debug!(priority = priority as u8, "Enqueued request");
            true
        } else {
            tracing::warn!("Queue is full, dropping request");
            false
        }
    }

    pub fn metrics(&self) -> QueueMetrics {
        let times = self.times.lock().unwrap();
        let avg_ms = if times.is_empty() {
            0.0
        } else {
            times.iter().map(|d| d.as_secs_f64() * 1000.0).sum::<f64>() / times.len() as f64
        };
        QueueMetrics {
            queued_requests: self.queue.count(),
            processing_requests: self.active_count.load(Ordering::Relaxed),
            completed_requests: self.total_processed.load(Ordering::Relaxed),
            failed_requests: self.total_failed.load(Ordering::Relaxed),
            average_processing_time: Span::new().milliseconds(avg_ms.round() as i64),
            last_updated: Timestamp::now(),
        }
    }

    async fn run(self: Arc<Self>) {
        tracing::info!("Queue processing started");
        loop {
            let Some(request) = self.queue.take(&self.cancel).await else {
                break;
            };
            // Backpressure: block the loop until a worker slot frees up.
            let permit = tokio::select! {
                _ = wait_cancel(&self.cancel) => break,
                p = self.workers.clone().acquire_owned() => match p { Ok(p) => p, Err(_) => break },
            };
            let this = Arc::clone(&self);
            let handle = tokio::spawn(async move {
                this.process_request(request).await;
                drop(permit); // semaphore release, as in C# finally
            });
            self.active_count.fetch_add(1, Ordering::Relaxed);
            self.active.lock().unwrap().push(handle);
        }
        tracing::info!("Queue processing stopped");
    }

    /// Port of ProcessRequestAsync. C# timing semantics: processing time is
    /// recorded on success and on a null inference result, but not on
    /// no-frames / db-error / cancellation.
    async fn process_request(&self, request: InferenceRequest) {
        let start = Instant::now();
        tracing::info!(request_id = %request.id, reason = %request.trigger_reason, "Processing request");
        let outcome: Result<bool, ()> = async {
            let frames = match capture_frames(&self.capture_cfg, &self.cancel).await {
                Some(f) if !f.is_empty() => f,
                _ => {
                    tracing::warn!(request_id = %request.id, "No frames captured");
                    return Err(()); // no time recorded, as in C#
                }
            };
            match self.ollama.analyze_frames(&frames, &self.cancel).await {
                Some(mut result) => {
                    result.request_id = request.id; // queue assigns the id, as in C#
                    let db = self.db.lock().unwrap();
                    match db.save_inference_result(&result) {
                        Ok(()) => {
                            tracing::info!(
                                request_id = %request.id,
                                activity = %result.activity_label,
                                confidence = result.confidence,
                                "Successfully processed request"
                            );
                            Ok(true)
                        }
                        Err(e) => {
                            tracing::error!(request_id = %request.id, error = %e, "Error saving inference result");
                            Err(())
                        }
                    }
                }
                None => {
                    if self.cancel.load(Ordering::Relaxed) {
                        tracing::warn!(request_id = %request.id, "Request was cancelled");
                        return Err(()); // cancellation: failed, no time, as in C#
                    }
                    tracing::warn!(request_id = %request.id, "Inference returned null result");
                    Ok(false) // failed, but time is recorded, as in C#
                }
            }
        }
        .await;
        match outcome {
            Ok(true) => {
                self.total_processed.fetch_add(1, Ordering::Relaxed);
                self.record_time(start.elapsed());
            }
            Ok(false) => {
                self.total_failed.fetch_add(1, Ordering::Relaxed);
                self.record_time(start.elapsed());
            }
            Err(()) => {
                self.total_failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.active_count.fetch_sub(1, Ordering::Relaxed);
    }

    fn record_time(&self, d: Duration) {
        let mut t = self.times.lock().unwrap();
        t.push_back(d);
        while t.len() > MAX_PROCESSING_TIME_SAMPLES {
            t.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ActivityMonitorSettings;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use uuid::Uuid;

    fn req(priority: RequestPriority) -> InferenceRequest {
        InferenceRequest {
            id: Uuid::new_v4(),
            timestamp: Timestamp::now(),
            trigger_reason: "test".into(),
            priority,
            captured_frames: None,
            metadata: None,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn priority_order_and_fifo_within_level() {
        let q = PriorityQueue::new(10);
        let cancel = AtomicBool::new(false);
        let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        // enqueue out of order: Low, Normal, High, Critical, Normal
        let mk = |p, id| InferenceRequest { id, priority: p, ..req(p) };
        q.try_add(mk(RequestPriority::Low, ids[0]), RequestPriority::Low);
        q.try_add(mk(RequestPriority::Normal, ids[1]), RequestPriority::Normal);
        q.try_add(mk(RequestPriority::High, ids[2]), RequestPriority::High);
        q.try_add(mk(RequestPriority::Critical, ids[3]), RequestPriority::Critical);
        q.try_add(mk(RequestPriority::Normal, ids[4]), RequestPriority::Normal);
        let mut got = Vec::new();
        for _ in 0..5 {
            got.push(q.take(&cancel).await.unwrap().id);
        }
        assert_eq!(got, vec![ids[3], ids[2], ids[1], ids[4], ids[0]]); // Critical, High, Normal FIFO, Low
        assert_eq!(q.count(), 0);
        cancel.store(true, Ordering::Relaxed);
        assert!(q.take(&cancel).await.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn drop_when_full_and_capacity_frees_on_take() {
        let q = PriorityQueue::new(2);
        let cancel = AtomicBool::new(false);
        assert!(q.try_add(req(RequestPriority::Normal), RequestPriority::Normal));
        assert!(q.try_add(req(RequestPriority::Normal), RequestPriority::Normal));
        assert!(!q.try_add(req(RequestPriority::Critical), RequestPriority::Critical)); // dropped
        assert_eq!(q.count(), 2);
        assert!(q.take(&cancel).await.is_some());
        assert!(q.try_add(req(RequestPriority::Critical), RequestPriority::Critical)); // room again
        assert_eq!(q.count(), 2);
    }

    /// Serve exactly one HTTP request (the /api/chat call).
    async fn one_chat_server(content: &'static str) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let h = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req: Vec<u8> = Vec::new();
            let mut buf = [0u8; 65536];
            loop {
                let n = sock.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
                if let Some(pos) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&req[..pos]).to_lowercase();
                    let cl = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if req.len() >= pos + 4 + cl {
                        break;
                    }
                }
            }
            let body = json!({"message": {"role": "assistant", "content": content}, "done": true}).to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
        });
        (format!("http://{addr}"), h)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn manager_end_to_end_one_request() {
        let _serial = crate::CAPTURE_TEST_LOCK.lock().unwrap(); // serialize with capture tests
        let inner = r#"{"activity_label":"Coding","application":"Comet","content_type":"code","topic":"t","action":"coding","summary":"s","visible_text":"v","confidence":0.9}"#;
        let (ep, server) = one_chat_server(inner).await;
        let mut s: ActivityMonitorSettings = serde_json::from_str("{}").unwrap();
        s.ollama_endpoint = ep;
        s.ollama_model = "m".into();
        s.queue_settings.processing_timeout_seconds = 30;
        s.capture_settings.frame_rate = 1;
        s.capture_settings.max_duration_seconds = 1;
        s.capture_settings.max_frames_per_capture = 1;
        let dir = std::env::temp_dir().join(format!("am-queue-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = ActivityDatabase::open(&dir.join("t.db")).unwrap();
        db.initialize().unwrap();
        let db = Arc::new(Mutex::new(db));
        let mgr = Arc::new(RequestQueueManager::new(&s, Arc::new(OllamaClient::new(&s)), db));
        mgr.start();
        assert!(mgr.enqueue(req(RequestPriority::High)));
        for _ in 0..100 {
            if mgr.metrics().completed_requests == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        mgr.stop().await;
        server.abort();
        let m = mgr.metrics();
        assert_eq!(m.completed_requests, 1, "metrics: {m:?}");
        assert_eq!(m.failed_requests, 0);
        assert_eq!(m.processing_requests, 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn manager_drop_when_full() {
        let mut s: ActivityMonitorSettings = serde_json::from_str("{}").unwrap();
        s.ollama_endpoint = "http://127.0.0.1:1".into(); // never called: queue is full
        s.queue_settings.max_queue_size = 1;
        let dir = std::env::temp_dir().join(format!("am-queue-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(Mutex::new(ActivityDatabase::open(&dir.join("t.db")).unwrap()));
        let mgr = Arc::new(RequestQueueManager::new(&s, Arc::new(OllamaClient::new(&s)), db));
        // don't start: items stay queued
        assert!(mgr.enqueue(req(RequestPriority::Low)));
        assert!(!mgr.enqueue(req(RequestPriority::Critical)));
        assert_eq!(mgr.metrics().queued_requests, 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
