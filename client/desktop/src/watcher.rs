use dioxus::prelude::*;

/// Reactive signal holding the last-seen config-change timestamp.
///
/// Shared between the watcher and callers of [`mark_self_written`] so that
/// config writes originating from the desktop itself are not treated as
/// external changes.
static LAST_SEEN: GlobalSignal<u64> = Signal::global(|| {
    savhub_local::pilot::config_change_timestamp().unwrap_or(0)
});

/// A Dioxus hook that polls `~/.savhub/.config-changed` every 2 seconds.
///
/// Returns a reactive signal that increments whenever an **external** config
/// change is detected (timestamp in the signal file is newer than what
/// we last observed).
pub fn use_config_watcher() -> Signal<u64> {
    let mut version = use_signal(|| 0u64);

    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let last = *LAST_SEEN.read();
            if savhub_local::pilot::has_config_changed_since(last) {
                let ts = savhub_local::pilot::config_change_timestamp().unwrap_or(0);
                *LAST_SEEN.write() = ts;
                version += 1;
            }
        }
    });

    version
}

/// Call this right after the desktop writes config so the watcher does not
/// treat the resulting signal-file bump as an external change.
pub fn mark_self_written() {
    // Give a tiny delay for the filesystem write to settle, then snapshot
    // the current signal timestamp.
    if let Some(ts) = savhub_local::pilot::config_change_timestamp() {
        *LAST_SEEN.write() = ts;
    }
}
