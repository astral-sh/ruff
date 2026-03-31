//! TSP request and notification handlers.

use crate::session::Session;
use crate::session::client::Client;
use crate::tsp::requests::{SnapshotChanged, SnapshotChangedParams};

pub(crate) mod search_paths;
pub(crate) mod snapshot;
pub(crate) mod version;

/// Sends a `typeServer/snapshotChanged` notification to the client if the
/// session revision has changed since `old_revision` and the client has
/// registered as a TSP consumer.
pub(crate) fn send_snapshot_changed_if_needed(
    old_revision: u64,
    session: &Session,
    client: &Client,
) {
    if !session.is_tsp_consumer() {
        return;
    }
    let new_revision = session.revision();
    if new_revision != old_revision {
        client.send_notification::<SnapshotChanged>(SnapshotChangedParams {
            old: old_revision,
            new: new_revision,
        });
    }
}
