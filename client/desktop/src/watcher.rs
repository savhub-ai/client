use dioxus::prelude::*;

/// A Dioxus hook that polls `~/.savhub/.config-changed` every 2 seconds.
///
/// Returns a reactive signal that increments whenever an external config
/// change is detected (timestamp in the signal file is newer than what
/// we last observed).
pub fn use_config_watcher() -> Signal<u64> {
    let mut version = use_signal(|| 0u64);
    let mut last_seen = use_signal(|| savhub_local::pilot::config_change_timestamp().unwrap_or(0));

    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            if savhub_local::pilot::has_config_changed_since(*last_seen.read()) {
                let ts = savhub_local::pilot::config_change_timestamp().unwrap_or(0);
                last_seen.set(ts);
                version += 1;
            }
        }
    });

    version
}
