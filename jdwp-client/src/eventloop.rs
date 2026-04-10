// JDWP Event Loop
//
// Handles concurrent reading of events and replies from JDWP socket

use crate::events::{parse_event_packet, EventSet};
use crate::protocol::{CommandPacket, JdwpError, JdwpResult, ReplyPacket, HEADER_SIZE, REPLY_FLAG};
use bytes::BytesMut;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, oneshot, Notify};
use tracing::{debug, error, info, warn};

/// Maximum allowed JDWP packet size (50MB)
/// AllClasses on large apps (Tomcat, Spring) can return 10-30MB of class signatures
const MAX_PACKET_SIZE: usize = 50 * 1024 * 1024;

/// Maximum time to wait for a command reply before considering it lost
const REPLY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Request to send a command and get reply
pub struct CommandRequest {
    pub packet: CommandPacket,
    pub reply_tx: oneshot::Sender<JdwpResult<ReplyPacket>>,
}

/// Handle to the event loop for sending commands and receiving events.
///
/// This handle can be cloned to send commands from multiple tasks, but only ONE clone
/// should call `recv_event()` or `try_recv_event()` at a time. The event receiver is
/// wrapped in an Arc<Mutex<Receiver>> which allows sharing, but concurrent event
/// consumption from multiple tasks will lead to unpredictable behavior (events distributed
/// round-robin across consumers).
///
/// # Thread Safety
/// - Commands can be sent concurrently from multiple clones
/// - Events should be consumed from a single task/clone
///
/// # Example
/// ```ignore
/// // Good: Single event consumer
/// let handle1 = event_loop.clone();
/// let handle2 = event_loop.clone();
///
/// // Both can send commands
/// handle1.send_command(cmd1);
/// handle2.send_command(cmd2);
///
/// // Only one should consume events
/// while let Some(event) = handle1.recv_event().await {
///     // Process event
/// }
/// ```
#[derive(Clone, Debug)]
pub struct EventLoopHandle {
    command_tx: mpsc::Sender<CommandRequest>,
    event_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<EventSet>>>,
    event_notify: Arc<Notify>,
}

impl EventLoopHandle {
    /// Send a command and wait for reply
    pub async fn send_command(&self, packet: CommandPacket) -> JdwpResult<ReplyPacket> {
        let (reply_tx, reply_rx) = oneshot::channel();

        let request = CommandRequest { packet, reply_tx };

        self.command_tx
            .send(request)
            .await
            .map_err(|_| JdwpError::Protocol("Event loop shut down".to_string()))?;

        reply_rx
            .await
            .map_err(|_| JdwpError::Protocol("Reply channel closed".to_string()))?
    }

    /// Try to receive an event (non-blocking).
    /// Returns `None` immediately if no events are available.
    pub async fn try_recv_event(&self) -> Option<EventSet> {
        let mut rx = self.event_rx.lock().await;
        rx.try_recv().ok()
    }

    /// Wait for the next event (blocking).
    ///
    /// Does not hold the channel lock across the await point, so concurrent
    /// `try_recv_event` calls will not be blocked.
    ///
    /// # Single-consumer invariant
    /// Only one task may call `recv_event` concurrently. The event loop uses
    /// `Notify::notify_one` which stores at most one permit, so the "try_recv
    /// returns Empty, then an event arrives before `notified().await` subscribes"
    /// race is impossible: the stored permit is consumed on the next subscribe.
    pub async fn recv_event(&self) -> Option<EventSet> {
        loop {
            // Brief lock to attempt a receive
            {
                let mut rx = self.event_rx.lock().await;
                match rx.try_recv() {
                    Ok(event) => return Some(event),
                    Err(mpsc::error::TryRecvError::Disconnected) => return None,
                    Err(mpsc::error::TryRecvError::Empty) => {}
                }
            }
            // Lock released — wait for notification that a new event was enqueued.
            // With notify_one(), a notification sent between lock release and
            // this subscribe is stored as a permit and consumed here immediately.
            self.event_notify.notified().await;
        }
    }
}

/// Start the event loop task
pub fn spawn_event_loop(reader: OwnedReadHalf, writer: OwnedWriteHalf) -> EventLoopHandle {
    let (command_tx, command_rx) = mpsc::channel(32);
    let (event_tx, event_rx) = mpsc::channel(256);
    let event_notify = Arc::new(Notify::new());

    let notify_clone = event_notify.clone();
    tokio::spawn(event_loop_task(
        reader,
        writer,
        command_rx,
        event_tx,
        notify_clone,
    ));

    EventLoopHandle {
        command_tx,
        event_rx: Arc::new(tokio::sync::Mutex::new(event_rx)),
        event_notify,
    }
}

/// Pending reply with timestamp for timeout tracking
struct PendingReply {
    sender: oneshot::Sender<JdwpResult<ReplyPacket>>,
    sent_at: tokio::time::Instant,
}

/// Main event loop task
async fn event_loop_task(
    mut reader: OwnedReadHalf,
    mut writer: OwnedWriteHalf,
    mut command_rx: mpsc::Receiver<CommandRequest>,
    event_tx: mpsc::Sender<EventSet>,
    event_notify: Arc<Notify>,
) {
    info!("Event loop started");

    let mut pending_replies: HashMap<u32, PendingReply> = HashMap::new();
    let mut cleanup_interval = tokio::time::interval(tokio::time::Duration::from_secs(10));

    loop {
        tokio::select! {
            // Handle outgoing commands
            Some(cmd) = command_rx.recv() => {
                let packet_id = cmd.packet.id;
                debug!("Sending command id={}", packet_id);

                let encoded = match cmd.packet.encode() {
                    Ok(data) => data,
                    Err(e) => {
                        cmd.reply_tx.send(Err(e)).ok();
                        continue;
                    }
                };
                if let Err(e) = writer.write_all(&encoded).await {
                    error!("Failed to write command: {}", e);
                    cmd.reply_tx.send(Err(JdwpError::Io(e))).ok();
                    continue;
                }

                if let Err(e) = writer.flush().await {
                    error!("Failed to flush command: {}", e);
                    cmd.reply_tx.send(Err(JdwpError::Io(e))).ok();
                    continue;
                }

                pending_replies.insert(packet_id, PendingReply {
                    sender: cmd.reply_tx,
                    sent_at: tokio::time::Instant::now(),
                });
            }

            // Periodic cleanup of timed-out pending replies
            _ = cleanup_interval.tick() => {
                let now = tokio::time::Instant::now();
                let before_count = pending_replies.len();

                pending_replies.retain(|packet_id, pending| {
                    let elapsed = now.duration_since(pending.sent_at);
                    if elapsed > REPLY_TIMEOUT {
                        warn!("Command {} timed out after {:?}, removing from pending replies", packet_id, elapsed);
                        // Note: sender is dropped here, which will notify the waiting command
                        false
                    } else {
                        true
                    }
                });

                let removed = before_count - pending_replies.len();
                if removed > 0 {
                    warn!("Cleaned up {} timed-out pending replies", removed);
                }
            }

            // Handle incoming packets
            result = read_packet(&mut reader) => {
                match result {
                    Ok((is_reply, packet_id, data)) => {
                        if is_reply {
                            // It's a reply - route to waiting command
                            debug!("Received reply id={}", packet_id);

                            if let Some(pending) = pending_replies.remove(&packet_id) {
                                match ReplyPacket::decode(&data) {
                                    Ok(reply) => {
                                        pending.sender.send(Ok(reply)).ok();
                                    }
                                    Err(e) => {
                                        warn!("Failed to decode reply: {}", e);
                                        pending.sender.send(Err(e)).ok();
                                    }
                                }
                            } else {
                                warn!("Received reply for unknown command id={} (may have timed out)", packet_id);
                            }
                        } else {
                            // It's an event - parse and broadcast
                            debug!("Received event packet, len={}", data.len());

                            // Event packets have command_set and command in header
                            // Data starts after 11-byte header
                            let event_data = &data[HEADER_SIZE..];

                            match parse_event_packet(event_data) {
                                Ok(event_set) => {
                                    info!("Parsed event set: {} events, suspend_policy={}",
                                          event_set.events.len(), event_set.suspend_policy);

                                    // Send event with backpressure — blocks if channel is full,
                                    // which is correct for a debugger (events must not be lost)
                                    match publish_event(&event_tx, &event_notify, event_set).await {
                                        Ok(_) => {}
                                        Err(_) => {
                                            info!("Event receiver dropped, shutting down event loop");
                                            break;
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to parse event: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to read packet: {}", e);
                        break;
                    }
                }
            }
        }
    }

    info!("Event loop shutting down");
}

/// Send an event to the consumer channel and wake the waiter.
///
/// Uses `notify_one` (not `notify_waiters`) so that a wake-up sent while the
/// single consumer is between `try_recv` and `notified().await` is stored as
/// a permit rather than lost. See `EventLoopHandle::recv_event` for the
/// consumer side of this contract.
async fn publish_event(
    event_tx: &mpsc::Sender<EventSet>,
    event_notify: &Arc<Notify>,
    event_set: EventSet,
) -> Result<(), mpsc::error::SendError<EventSet>> {
    event_tx.send(event_set).await?;
    event_notify.notify_one();
    Ok(())
}

/// Read a packet from the socket and determine if it's a reply or event
async fn read_packet(reader: &mut OwnedReadHalf) -> JdwpResult<(bool, u32, Vec<u8>)> {
    // Read header
    let mut header = BytesMut::with_capacity(HEADER_SIZE);
    header.resize(HEADER_SIZE, 0);

    reader
        .read_exact(&mut header)
        .await
        .map_err(JdwpError::Io)?;

    // Parse header
    let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let packet_id = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    let flags = header[8];

    if length < HEADER_SIZE {
        return Err(JdwpError::Protocol(format!(
            "Invalid packet length: {}",
            length
        )));
    }

    if length > MAX_PACKET_SIZE {
        return Err(JdwpError::Protocol(format!(
            "Packet too large: {} bytes (max: {} bytes)",
            length, MAX_PACKET_SIZE
        )));
    }

    // Read rest of packet
    let data_len = length - HEADER_SIZE;
    let mut full_packet = header.to_vec();

    if data_len > 0 {
        let mut data = vec![0u8; data_len];
        reader.read_exact(&mut data).await.map_err(JdwpError::Io)?;
        full_packet.extend_from_slice(&data);
    }

    let is_reply = flags == REPLY_FLAG;

    Ok((is_reply, packet_id, full_packet))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Event, EventKind};

    impl EventLoopHandle {
        /// Test-only constructor that bypasses the TCP event loop.
        /// Returns the handle plus a producer half (event sender + notifier).
        fn new_for_test() -> (Self, mpsc::Sender<EventSet>, Arc<Notify>) {
            let (_command_tx, _command_rx) = mpsc::channel::<CommandRequest>(32);
            let (event_tx, event_rx) = mpsc::channel::<EventSet>(256);
            let event_notify = Arc::new(Notify::new());
            let handle = EventLoopHandle {
                command_tx: _command_tx,
                event_rx: Arc::new(tokio::sync::Mutex::new(event_rx)),
                event_notify: event_notify.clone(),
            };
            (handle, event_tx, event_notify)
        }
    }

    fn dummy_event(id: i32) -> EventSet {
        EventSet {
            suspend_policy: 0,
            events: vec![Event {
                kind: 90, // VM_START
                request_id: id,
                details: EventKind::VMStart { thread: 0 },
            }],
        }
    }

    /// Mirror of `recv_event`'s inner loop, but with two `Barrier` sync
    /// points between releasing the rx lock and subscribing to `notified()`.
    /// The first barrier signals "lock released", the second waits until
    /// the test has driven `publish_event` — this deterministically parks
    /// the consumer in the exact race window the bug required.
    ///
    /// We can't do this on real `recv_event` because it transitions from
    /// "lock released" to "notified subscribed" without an await point,
    /// so there is no externally-observable moment to fire the producer in.
    async fn recv_event_with_sync(
        handle: &EventLoopHandle,
        released: Arc<tokio::sync::Barrier>,
        published: Arc<tokio::sync::Barrier>,
    ) -> Option<EventSet> {
        loop {
            {
                let mut rx = handle.event_rx.lock().await;
                match rx.try_recv() {
                    Ok(event) => return Some(event),
                    Err(mpsc::error::TryRecvError::Disconnected) => return None,
                    Err(mpsc::error::TryRecvError::Empty) => {}
                }
            }
            released.wait().await;
            published.wait().await;
            handle.event_notify.notified().await;
        }
    }

    /// Regression test for issue #9 (dronsv/jdwp-mcp).
    ///
    /// `publish_event` must wake a consumer that is between releasing the
    /// rx lock and subscribing to `notified()`. With `notify_waiters()`
    /// the wake-up is lost (notify_waiters stores no permit); with
    /// `notify_one()` the wake-up is stored as a permit and consumed by
    /// the consumer's subsequent subscribe.
    ///
    /// The consumer runs `recv_event_with_sync` so we can deterministically
    /// park it between lock release and notified subscribe. `publish_event`
    /// is the real production helper — a regression that swaps `notify_one`
    /// → `notify_waiters` makes this test time out.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn publish_event_wakes_consumer_in_race_window() {
        let (handle, tx, _notify) = EventLoopHandle::new_for_test();
        let released = Arc::new(tokio::sync::Barrier::new(2));
        let published = Arc::new(tokio::sync::Barrier::new(2));

        let released_c = released.clone();
        let published_c = published.clone();
        let handle_c = handle.clone();
        let consumer = tokio::spawn(async move {
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                recv_event_with_sync(&handle_c, released_c, published_c),
            )
            .await
        });

        // Step 1: consumer releases rx lock and parks at `released`
        released.wait().await;

        // Step 2: fire the producer while consumer is NOT yet subscribed
        publish_event(&tx, &handle.event_notify, dummy_event(42))
            .await
            .unwrap();

        // Step 3: release the consumer; it now subscribes via notified().await
        // With notify_one: the permit stored in step 2 is consumed immediately.
        // With notify_waiters: the notification was lost; consumer hangs.
        published.wait().await;

        let received = consumer
            .await
            .expect("consumer panicked")
            .expect("recv_event hung in race window — publish_event regression");
        assert!(received.is_some(), "expected Some(event), got None");
    }
}
