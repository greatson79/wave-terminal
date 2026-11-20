use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::broadcast;

pub const CAPACITY: usize = 4 * 1024 * 1024;
const WRITE_CHUNK: usize = 16 * 1024;

struct Ring {
    bytes: VecDeque<u8>,
    end: u64,
}

pub struct Output {
    ring: Mutex<Ring>,
    wake: broadcast::Sender<()>,
    pub lagged_chunks: AtomicU64,
    pub gap_events: AtomicU64,
    pub gap_bytes: AtomicU64,
}

impl Output {
    pub fn new() -> Self {
        Self {
            ring: Mutex::new(Ring {
                bytes: VecDeque::new(),
                end: 0,
            }),
            wake: broadcast::channel(256).0,
            lagged_chunks: AtomicU64::new(0),
            gap_events: AtomicU64::new(0),
            gap_bytes: AtomicU64::new(0),
        }
    }

    pub fn publish(&self, bytes: &[u8]) {
        let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        ring.end = ring
            .end
            .checked_add(bytes.len() as u64)
            .expect("PTY output offset overflow");
        let retained = &bytes[bytes.len().saturating_sub(CAPACITY)..];
        let discard = (ring.bytes.len() + retained.len()).saturating_sub(CAPACITY);
        ring.bytes.drain(..discard);
        let needed = ring.bytes.len() + retained.len();
        if needed > ring.bytes.capacity() {
            let target = needed.next_power_of_two().min(CAPACITY);
            let additional = target - ring.bytes.len();
            ring.bytes.reserve_exact(additional);
        }
        ring.bytes.extend(retained);
        let _ = self.wake.send(());
    }

    // The caller holds the parser lock so this cursor and its screen snapshot agree.
    pub fn subscribe(&self) -> (broadcast::Receiver<()>, u64) {
        let ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        (self.wake.subscribe(), ring.end)
    }

    pub fn read(&self, cursor: u64) -> Result<Vec<u8>, u64> {
        let ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        let oldest = ring.end - ring.bytes.len() as u64;
        if cursor < oldest {
            return Err(oldest - cursor);
        }
        assert!(cursor <= ring.end, "attach cursor ahead of producer");
        let start = (cursor - oldest) as usize;
        let stop = (start + WRITE_CHUNK).min(ring.bytes.len());
        Ok(ring.bytes.range(start..stop).copied().collect())
    }

    fn record_lag(&self, surface_id: u64, n: u64) {
        self.lagged_chunks.fetch_add(n, Ordering::Relaxed);
        eprintln!("[attach lag] surface={surface_id} missed_notifications={n}");
    }
}

pub async fn pump<W: AsyncWrite + Unpin>(
    w: &mut W,
    rx: &mut broadcast::Receiver<()>,
    output: &Output,
    mut cursor: u64,
    surface_id: u64,
    exited: &AtomicBool,
) {
    loop {
        if let Err(broadcast::error::TryRecvError::Lagged(n)) = rx.try_recv() {
            output.record_lag(surface_id, n);
        }
        let bytes = match output.read(cursor) {
            Ok(bytes) => bytes,
            Err(missed) => {
                output.gap_events.fetch_add(1, Ordering::Relaxed);
                output.gap_bytes.fetch_add(missed, Ordering::Relaxed);
                eprintln!(
                    "[attach gap] surface={surface_id} missed_bytes={missed} capacity={CAPACITY}"
                );
                let marker = format!("\x18\x1b\\\x1b[0m\r\n[Wave output gap: {missed} bytes outside {CAPACITY}-byte history; re-attaching]\r\n");
                let _ = w.write_all(marker.as_bytes()).await;
                let _ = w.flush().await;
                return;
            }
        };
        if !bytes.is_empty() {
            if w.write_all(&bytes).await.is_err() || w.flush().await.is_err() {
                return;
            }
            cursor += bytes.len() as u64;
            continue;
        }
        tokio::select! {
            r = rx.recv() => match r {
                Ok(()) => {},
                Err(broadcast::error::RecvError::Lagged(n)) => output.record_lag(surface_id, n),
                Err(_) => return,
            },
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                if exited.load(Ordering::Relaxed) { return; }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn slow_attach_recovers_more_than_256_chunks_without_loss() {
        let output = Arc::new(Output::new());
        let (mut rx, cursor) = output.subscribe();
        let exited = Arc::new(AtomicBool::new(false));
        let (mut writer, mut reader) = tokio::io::duplex(16);
        let source = Arc::clone(&output);
        let stopped = Arc::clone(&exited);
        let task = tokio::spawn(async move {
            pump(&mut writer, &mut rx, &source, cursor, 77, &stopped).await;
        });
        let chunk = |sequence: u32| {
            let mut bytes = [sequence as u8; 64];
            bytes[..4].copy_from_slice(&sequence.to_le_bytes());
            bytes
        };
        let expected: Vec<u8> = (0..=600).flat_map(chunk).collect();
        output.publish(&chunk(0));
        while output.wake.len() != 0 {
            tokio::task::yield_now().await;
        }
        for sequence in 1..=600 {
            output.publish(&chunk(sequence));
        }
        exited.store(true, Ordering::Relaxed);
        let mut received = Vec::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            reader.read_to_end(&mut received),
        )
        .await
        .unwrap()
        .unwrap();
        task.await.unwrap();
        eprintln!(
            "slow actual pump: expected={} received={} lagged_chunks={}",
            601 * 64,
            received.len(),
            output.lagged_chunks.load(Ordering::Relaxed)
        );
        assert!(output.lagged_chunks.load(Ordering::Relaxed) > 0);
        assert_eq!(received.len(), 601 * 64);
        assert_eq!(received, expected);
        assert_eq!(output.gap_events.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn fresh_seat_allocates_history_only_as_output_arrives() {
        let output = Output::new();
        {
            let ring = output.ring.lock().unwrap();
            assert_eq!(ring.bytes.len(), 0);
            assert_eq!(ring.bytes.capacity(), 0);
        }
        output.publish(b"small");
        assert!(output.ring.lock().unwrap().bytes.capacity() < CAPACITY);
        for size in [257, 8193, 65537, 1048577, CAPACITY - 1, CAPACITY + 1] {
            output.publish(&vec![b'x'; size]);
            let ring = output.ring.lock().unwrap();
            assert!(ring.bytes.len() <= CAPACITY);
            assert!(ring.bytes.capacity() <= CAPACITY);
        }
    }

    #[test]
    fn lazy_ring_growth_never_reserves_past_cap_for_irregular_chunks() {
        let output = Output::new();
        output.publish(&vec![b'a'; 3 * CAPACITY / 4]);
        output.publish(&vec![b'b'; CAPACITY / 4]);
        assert_eq!(output.ring.lock().unwrap().bytes.len(), CAPACITY);
        assert!(output.ring.lock().unwrap().bytes.capacity() <= CAPACITY);

        let output = Output::new();
        output.publish(&[b'c'; 243]);
        for _ in 0..=CAPACITY / WRITE_CHUNK {
            output.publish(&[b'd'; WRITE_CHUNK]);
            assert!(output.ring.lock().unwrap().bytes.capacity() <= CAPACITY);
        }
    }

    #[test]
    fn ring_allocation_never_exceeds_4_mib_and_exact_cap_is_retained() {
        let output = Output::new();
        output.publish(&vec![b'a'; CAPACITY]);
        assert!(output.read(0).is_ok());
        output.publish(b"b");
        assert_eq!(output.read(0), Err(1));
        assert!(output.read(1).is_ok());
        output.publish(&vec![b'c'; CAPACITY + 123]);
        let ring = output.ring.lock().unwrap();
        assert_eq!(ring.bytes.len(), CAPACITY);
        assert_eq!(ring.bytes.capacity(), CAPACITY);
        assert!(ring.bytes.iter().all(|&byte| byte == b'c'));
    }

    #[tokio::test]
    async fn exactly_one_cap_of_backlog_is_delivered_without_a_gap() {
        let output = Arc::new(Output::new());
        let (mut rx, cursor) = output.subscribe();
        let exited = Arc::new(AtomicBool::new(false));
        let (mut writer, mut reader) = tokio::io::duplex(WRITE_CHUNK);
        let source = Arc::clone(&output);
        let stopped = Arc::clone(&exited);
        let task = tokio::spawn(async move {
            pump(&mut writer, &mut rx, &source, cursor, 81, &stopped).await;
        });
        output.publish(&[b'f'; 64]);
        while output.wake.len() != 0 {
            tokio::task::yield_now().await;
        }
        let payload: Vec<u8> = (0..CAPACITY).map(|n| n as u8).collect();
        output.publish(&payload);
        exited.store(true, Ordering::Relaxed);
        let mut received = Vec::new();
        // Slow-reader pressure has its own test; this case checks the full-cap boundary.
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            reader.read_to_end(&mut received),
        )
        .await
        .unwrap()
        .unwrap();
        task.await.unwrap();
        assert_eq!(received.len(), 64 + CAPACITY);
        assert_eq!(&received[..64], &[b'f'; 64]);
        assert_eq!(&received[64..], payload);
        assert_eq!(output.gap_events.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn cap_exceeded_is_visible_counted_and_closes_the_actual_stream() {
        let output = Arc::new(Output::new());
        let (mut rx, cursor) = output.subscribe();
        let exited = AtomicBool::new(false);
        let (mut writer, mut reader) = tokio::io::duplex(16);
        let source = Arc::clone(&output);
        let task = tokio::spawn(async move {
            pump(&mut writer, &mut rx, &source, cursor, 79, &exited).await;
        });
        output.publish(&[b'x'; 64]);
        while output.wake.len() != 0 {
            tokio::task::yield_now().await;
        }
        output.publish(&vec![b'y'; CAPACITY + 1]);
        let mut received = Vec::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            reader.read_to_end(&mut received),
        )
        .await
        .unwrap()
        .unwrap();
        task.await.unwrap();
        assert!(received.starts_with(&[b'x'; 64]));
        assert!(String::from_utf8_lossy(&received).contains("[Wave output gap: 1 bytes"));
        assert_eq!(output.gap_events.load(Ordering::Relaxed), 1);
        assert_eq!(output.gap_bytes.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn a_new_subscription_does_not_replay_prior_history() {
        let output = Output::new();
        output.publish(b"old");
        let (mut rx, cursor) = output.subscribe();
        output.publish(b"new");
        let (mut writer, mut reader) = tokio::io::duplex(16);
        let exited = AtomicBool::new(true);
        let (_, bytes) = tokio::join!(
            pump(&mut writer, &mut rx, &output, cursor, 80, &exited),
            async {
                let mut bytes = [0; 3];
                reader.read_exact(&mut bytes).await.unwrap();
                bytes
            }
        );
        assert_eq!(&bytes, b"new");
    }

    #[tokio::test]
    async fn mac_fixture_actual_attach_pump_preserves_exact_bytes() {
        let fixture =
            include_bytes!("../../../scripts/probes/scroll-history/fixtures/claude_scroll100.bin")
                .to_vec();
        let output = Output::new();
        let (mut rx, cursor) = output.subscribe();
        let exited = AtomicBool::new(true);
        let (mut writer, mut reader) = tokio::io::duplex(64);
        output.publish(&fixture);
        let ((), bytes) = tokio::join!(
            pump(&mut writer, &mut rx, &output, cursor, 78, &exited),
            async {
                let mut bytes = vec![0; fixture.len()];
                reader.read_exact(&mut bytes).await.unwrap();
                bytes
            }
        );
        assert_eq!(bytes, fixture);
        if let Ok(path) = std::env::var("SCROLL_CAPTURE_PATH") {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .unwrap()
                .write_all(&bytes)
                .unwrap();
            eprintln!("actual pump fixture saved: {path}");
        }
    }
}
