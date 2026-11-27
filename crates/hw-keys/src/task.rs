// SPDX-License-Identifier: GPL-3.0-or-later
//! Running blocking device and OS key store calls off the async executor.

use crate::error::{Error, Result};

/// Runs a blocking closure on tokio's blocking pool. A panic inside the closure (or a
/// runtime shutdown) becomes [`Error::Task`] instead of propagating.
pub(crate) async fn blocking<T, F>(f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| Error::Task)?
}
