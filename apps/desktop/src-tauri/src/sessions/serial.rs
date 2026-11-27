// SPDX-License-Identifier: GPL-3.0-or-later
//! Serial console sessions (desktop: native ports; Android: USB serial plugin transport).

use super::record::Recorder;
use super::{EventSink, Input, SessionPhase, SessionTarget};
use crate::error::{CmdError, CmdResult};
use tokio::sync::mpsc;

pub fn spawn(
    app: &tauri::AppHandle,
    id: String,
    target: SessionTarget,
    mut input: mpsc::UnboundedReceiver<Input>,
    sink: EventSink,
) -> CmdResult<()> {
    let SessionTarget::Serial {
        path,
        baud_rate,
        data_bits,
        stop_bits,
        parity,
        flow_control,
    } = target
    else {
        return Err(CmdError::invalid("not a serial target"));
    };
    let config: serial_io::SerialConfig = serde_json::from_value(serde_json::json!({
        "baudRate": baud_rate,
        "dataBits": data_bits,
        "stopBits": stop_bits,
        "parity": parity,
        "flowControl": flow_control,
    }))
    .map_err(|e| CmdError::invalid(format!("invalid serial settings: {e}")))?;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (session, mut events) = match open_port(&path, &config).await {
            Ok(s) => s,
            Err(e) => {
                sink.closed(Some(e.to_string()), true);
                super::finish(&app, &id);
                return;
            }
        };
        sink.status(SessionPhase::Connected);
        let mut rec = Recorder::start(&app, None, &path, 80, 24);
        loop {
            tokio::select! {
                ev = events.recv() => match ev {
                    Some(serial_io::Event::Output(d)) => {
                        rec.output(&d);
                        sink.output(&d);
                    }
                    Some(serial_io::Event::Closed { reason }) => {
                        sink.closed(Some(format!("{reason:?}")), false);
                        break;
                    }
                    None => {
                        sink.closed(None, false);
                        break;
                    }
                },
                msg = input.recv() => match msg {
                    Some(Input::Data(d)) => { let _ = session.write(&d).await; }
                    Some(Input::Resize { .. }) => {}
                    Some(Input::Close) | None => {
                        let _ = session.close().await;
                        sink.closed(None, false);
                        break;
                    }
                }
            }
        }
        rec.finish();
        super::finish(&app, &id);
    });
    Ok(())
}

/// Opens a native serial port.
#[cfg(desktop)]
async fn open_port(
    path: &str,
    config: &serial_io::SerialConfig,
) -> serial_io::Result<(serial_io::Session, mpsc::Receiver<serial_io::Event>)> {
    serial_io::Session::open(path, config).await
}

/// Mobile devices have no native serial ports; USB serial adapters need a platform transport
/// that does not exist yet.
#[cfg(mobile)]
async fn open_port(
    _path: &str,
    _config: &serial_io::SerialConfig,
) -> serial_io::Result<(serial_io::Session, mpsc::Receiver<serial_io::Event>)> {
    Err(serial_io::Error::Unsupported(
        "serial consoles are not available on this device yet",
    ))
}
