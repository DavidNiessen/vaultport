// SPDX-License-Identifier: GPL-3.0-or-later
//! Session tests over an in-memory transport (the same path an Android USB serial plugin
//! takes).

use std::sync::atomic::AtomicUsize;

use tokio::time::timeout;

use super::*;

const WAIT: Duration = Duration::from_secs(5);

/// In-memory duplex: the test feeds "device output" through `device` and inspects what the
/// session wrote and which control operations it performed.
#[derive(Debug)]
struct MemoryTransport {
    incoming: Mutex<mpsc::Receiver<io::Result<Option<Vec<u8>>>>>,
    ops: Arc<std::sync::Mutex<Vec<String>>>,
    closes: Arc<AtomicUsize>,
}

struct Device {
    tx: mpsc::Sender<io::Result<Option<Vec<u8>>>>,
    ops: Arc<std::sync::Mutex<Vec<String>>>,
    closes: Arc<AtomicUsize>,
}

impl Device {
    async fn send(&self, item: io::Result<Option<Vec<u8>>>) {
        self.tx.send(item).await.unwrap();
    }

    fn ops(&self) -> Vec<String> {
        self.ops.lock().unwrap().clone()
    }

    fn closes(&self) -> usize {
        self.closes.load(Ordering::SeqCst)
    }
}

fn memory_transport() -> (MemoryTransport, Device) {
    let (tx, rx) = mpsc::channel(1024);
    let ops = Arc::new(std::sync::Mutex::new(Vec::new()));
    let closes = Arc::new(AtomicUsize::new(0));
    (
        MemoryTransport {
            incoming: Mutex::new(rx),
            ops: Arc::clone(&ops),
            closes: Arc::clone(&closes),
        },
        Device { tx, ops, closes },
    )
}

#[async_trait::async_trait]
impl SerialTransport for MemoryTransport {
    async fn read(&self) -> io::Result<Option<Vec<u8>>> {
        match self.incoming.lock().await.recv().await {
            Some(item) => item,
            None => Ok(None),
        }
    }

    async fn write(&self, data: &[u8]) -> io::Result<()> {
        self.ops
            .lock()
            .unwrap()
            .push(format!("write {}", String::from_utf8_lossy(data)));
        Ok(())
    }

    async fn send_break(&self, duration: Duration) -> io::Result<()> {
        self.ops
            .lock()
            .unwrap()
            .push(format!("break {}", duration.as_millis()));
        Ok(())
    }

    async fn set_dtr(&self, level: bool) -> io::Result<()> {
        self.ops.lock().unwrap().push(format!("dtr {level}"));
        Ok(())
    }

    async fn set_rts(&self, level: bool) -> io::Result<()> {
        if level {
            self.ops.lock().unwrap().push("rts true".to_owned());
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Unsupported, "no rts"))
        }
    }

    async fn close(&self) -> io::Result<()> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

async fn next(events: &mut mpsc::Receiver<Event>) -> Event {
    timeout(WAIT, events.recv()).await.unwrap().unwrap()
}

#[tokio::test]
async fn output_writes_and_control_lines() {
    let (transport, device) = memory_transport();
    let (session, mut events) = Session::from_transport(Box::new(transport)).unwrap();

    device.send(Ok(Some(b"login: ".to_vec()))).await;
    device.send(Ok(Some(Vec::new()))).await; // empty chunks are skipped
    device.send(Ok(Some(b"\r\n".to_vec()))).await;
    assert_eq!(next(&mut events).await, Event::Output(b"login: ".to_vec()));
    assert_eq!(next(&mut events).await, Event::Output(b"\r\n".to_vec()));

    session.write(b"root\r").await.unwrap();
    session
        .send_break(Duration::from_millis(250))
        .await
        .unwrap();
    session.set_dtr(false).await.unwrap();
    session.set_rts(true).await.unwrap();
    assert!(matches!(
        session.set_rts(false).await,
        Err(Error::Unsupported(_))
    ));
    assert_eq!(
        device.ops(),
        ["write root\r", "break 250", "dtr false", "rts true"]
    );

    assert!(matches!(
        session.send_break(Duration::ZERO).await,
        Err(Error::InvalidConfig(_))
    ));
    assert!(matches!(
        session.send_break(Duration::from_secs(11)).await,
        Err(Error::InvalidConfig(_))
    ));

    let clone = session.clone();
    clone.close().await.unwrap();
    assert!(session.is_closed());
    assert_eq!(
        next(&mut events).await,
        Event::Closed {
            reason: CloseReason::Local
        }
    );
    assert!(timeout(WAIT, events.recv()).await.unwrap().is_none());
    assert_eq!(device.closes(), 1);

    assert!(matches!(session.write(b"x").await, Err(Error::Closed)));
    assert!(matches!(session.set_dtr(true).await, Err(Error::Closed)));
    session.close().await.unwrap(); // idempotent
    assert_eq!(device.closes(), 1);
}

#[tokio::test]
async fn device_disconnect_and_errors_end_the_session() {
    let cases = [
        (Ok(None), CloseReason::Disconnected),
        (
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "hup")),
            CloseReason::Disconnected,
        ),
        (
            Err(io::Error::from_raw_os_error(5)),
            if cfg!(unix) {
                CloseReason::Disconnected
            } else {
                CloseReason::Error {
                    message: io::Error::from_raw_os_error(5).to_string(),
                }
            },
        ),
        (
            Err(io::Error::other("framing error storm")),
            CloseReason::Error {
                message: "framing error storm".to_owned(),
            },
        ),
    ];
    for (item, expected) in cases {
        let (transport, device) = memory_transport();
        let (session, mut events) = Session::from_transport(Box::new(transport)).unwrap();
        device.send(item).await;
        assert_eq!(next(&mut events).await, Event::Closed { reason: expected });
        assert!(session.is_closed());
        assert_eq!(device.closes(), 1);
        assert!(matches!(session.write(b"x").await, Err(Error::Closed)));
    }
}

#[tokio::test]
async fn dropping_every_handle_closes_the_transport() {
    let (transport, device) = memory_transport();
    let (session, mut events) = Session::from_transport(Box::new(transport)).unwrap();
    drop(session);
    assert_eq!(
        next(&mut events).await,
        Event::Closed {
            reason: CloseReason::Local
        }
    );
    assert_eq!(device.closes(), 1);
}

#[tokio::test]
async fn dropping_the_receiver_ends_the_session() {
    let (transport, device) = memory_transport();
    let (session, events) = Session::from_transport(Box::new(transport)).unwrap();
    drop(events);
    device.send(Ok(Some(b"data".to_vec()))).await;
    timeout(WAIT, async {
        while !session.is_closed() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(device.closes(), 1);
}

#[tokio::test]
async fn close_does_not_wait_for_a_slow_consumer() {
    let (transport, device) = memory_transport();
    let (session, mut events) = Session::from_transport(Box::new(transport)).unwrap();
    for i in 0..(EVENT_CHANNEL_CAPACITY + 10) {
        device.send(Ok(Some(vec![(i % 256) as u8]))).await;
    }
    // Let the reader fill the channel, then close without draining it.
    tokio::time::sleep(Duration::from_millis(50)).await;
    timeout(WAIT, session.close()).await.unwrap().unwrap();
    assert_eq!(device.closes(), 1);

    // Everything queued is still delivered, followed by `Closed`.
    let mut outputs = 0;
    loop {
        match next(&mut events).await {
            Event::Output(_) => outputs += 1,
            Event::Closed { reason } => {
                assert_eq!(reason, CloseReason::Local);
                break;
            }
        }
    }
    assert_eq!(outputs, EVENT_CHANNEL_CAPACITY);
}

#[test]
fn from_transport_requires_a_runtime() {
    let (transport, _device) = memory_transport();
    assert!(matches!(
        Session::from_transport(Box::new(transport)),
        Err(Error::NoRuntime)
    ));
}

#[test]
fn close_reason_serializes_tagged() {
    assert_eq!(
        serde_json::to_value(CloseReason::Error {
            message: "x".into()
        })
        .unwrap(),
        serde_json::json!({"type": "error", "message": "x"})
    );
    assert_eq!(
        serde_json::to_value(CloseReason::Disconnected).unwrap(),
        serde_json::json!({"type": "disconnected"})
    );
}
