use beacon_chain::{BeaconChain, BeaconChainTypes};
use environment::RuntimeContext;
use exit_future::Signal;
use futures::{Future, Stream};
use parking_lot::Mutex;
use slog::{debug, error, info};
use slot_clock::SlotClock;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::timer::Interval;
use types::{EthSpec, Slot};

const SECS_PER_MINUTE: u64 = 60;
const SECS_PER_HOUR: u64 = 3600;
const SECS_PER_DAY: u64 = 86400; // non-leap
const SECS_PER_WEEK: u64 = 604800; // non-leap
const DAYS_PER_WEEK: u64 = 7;
const HOURS_PER_DAY: u64 = 24;
const MINUTES_PER_HOUR: u64 = 60;

pub fn spawn_slot_notifier<T: BeaconChainTypes>(
    context: RuntimeContext<T::EthSpec>,
    beacon_chain: Arc<BeaconChain<T>>,
    milliseconds_per_slot: u64,
) -> Result<Signal, String> {
    let log_1 = context.log.clone();
    let log_2 = context.log.clone();

    let slot_duration = Duration::from_millis(milliseconds_per_slot);
    let duration_to_next_slot = beacon_chain
        .slot_clock
        .duration_to_next_slot()
        .ok_or_else(|| "slot_notifier unable to determine time to next slot")?;

    // Run this half way through each slot.
    let start_instant = Instant::now() + duration_to_next_slot + (slot_duration / 2);

    // Run this each slot.
    let interval_duration = slot_duration;

    let previous_head_slot = Mutex::new(Slot::new(0));

    let interval_future = Interval::new(start_instant, interval_duration)
        .map_err(
            move |e| error!(log_1, "Slot notifier timer failed"; "error" => format!("{:?}", e)),
        )
        .for_each(move |_| {
            let head = beacon_chain.head();

            let head_slot = head.beacon_block.slot;
            let head_epoch = head_slot.epoch(T::EthSpec::slots_per_epoch());
            let current_slot = beacon_chain.slot().map_err(|e| {
                error!(
                    log_2,
                    "Unable to read current slot";
                    "error" => format!("{:?}", e)
                )
            })?;
            let current_epoch = current_slot.epoch(T::EthSpec::slots_per_epoch());
            let finalized_epoch = head.beacon_state.finalized_checkpoint.epoch;
            let finalized_root = head.beacon_state.finalized_checkpoint.root;
            let head_root = head.beacon_block_root;

            let mut previous_head_slot = previous_head_slot.lock();

            // The next two lines take advantage of saturating subtraction on `Slot`.
            let head_distance = current_slot - head_slot;
            let slots_since_last_update = head_slot - *previous_head_slot;

            *previous_head_slot = head_slot;

            debug!(
                log_2,
                "Slot timer";
                "finalized_root" => format!("{}", finalized_root),
                "finalized_epoch" => finalized_epoch,
                "head_block" => format!("{}", head_root),
                "head_slot" => head_slot,
                "current_slot" => current_slot,
            );

            if head_epoch + 1 < current_epoch {
                let distance = format!(
                    "{} slots ({})",
                    head_distance.as_u64(),
                    slot_distance_pretty(head_distance, slot_duration)
                );

                info!(
                    log_2,
                    "Syncing";
                    "speed" => sync_rate_pretty(slots_since_last_update, interval_duration.as_secs()),
                    "distance" => distance
                );

                return Ok(());
            };

            macro_rules! not_quite_synced_log {
                ($message: expr) => {
                    info!(
                        log_2,
                        $message;
                        "finalized_root" => format!("{}", finalized_root),
                        "finalized_epoch" => finalized_epoch,
                        "head_slot" => head_slot,
                        "current_slot" => current_slot,
                    );
                }
            }

            if head_epoch + 1 == current_epoch {
                not_quite_synced_log!("Synced to previous epoch")
            } else if head_slot != current_slot {
                not_quite_synced_log!("Synced to current epoch")
            } else {
                info!(
                    log_2,
                    "Synced";
                    "finalized_root" => format!("{}", finalized_root),
                    "finalized_epoch" => finalized_epoch,
                    "epoch" => current_epoch,
                    "slot" => current_slot,
                );
            };

            Ok(())
        });

    let (exit_signal, exit) = exit_future::signal();
    context
        .executor
        .spawn(exit.until(interval_future).map(|_| ()));

    Ok(exit_signal)
}

fn sync_rate_pretty(slots_since_last_update: Slot, update_interval_secs: u64) -> String {
    if update_interval_secs == 0 {
        return "Error".into();
    }

    if slots_since_last_update == 0 {
        "No progress".into()
    } else {
        format!(
            "{} slots/sec",
            slots_since_last_update / update_interval_secs
        )
    }
}

fn slot_distance_pretty(slot_span: Slot, slot_duration: Duration) -> String {
    if slot_duration == Duration::from_secs(0) {
        return String::from("Unknown");
    }

    let secs = (slot_duration * slot_span.as_u64() as u32).as_secs();

    let weeks = secs / SECS_PER_WEEK;
    let days = secs / SECS_PER_DAY;
    let hours = secs / SECS_PER_HOUR;
    let minutes = secs / SECS_PER_MINUTE;

    if weeks > 0 {
        format!("{} weeks {} days", weeks, days % DAYS_PER_WEEK)
    } else if days > 0 {
        format!("{} days {} hrs", days, hours % HOURS_PER_DAY)
    } else if hours > 0 {
        format!("{} hrs {} mins", hours, minutes % MINUTES_PER_HOUR)
    } else {
        format!("{} mins", minutes)
    }
}
