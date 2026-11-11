//! Keeping graphics cards asleep.
//!
//! amdgpu restarts a card's runtime-suspend timer whenever a sysfs read
//! reaches the hardware (load, temperature, clock, power). Read every second,
//! an idle card that could suspend never would, and a laptop's discrete GPU
//! would burn watts all day. So a card that could suspend is read in short
//! bursts: a burst opens a window of half a second, then the card is left
//! alone for its autosuspend delay plus a margin, and callers repeat what they
//! read last. A card that drives a lit display cannot suspend anyway and is
//! read freely; a suspended card is never touched.

use crate::sys;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

const BURST: Duration = Duration::from_millis(500);
const MARGIN: Duration = Duration::from_secs(1);
const RECHECK: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Power {
    /// Never suspends (or drives a display): read at will.
    Free,
    /// Awake but would suspend after this long untouched.
    Idleable(Duration),
    /// Suspended: reading would wake it.
    Asleep,
}

struct Card {
    power: Power,
    checked: Instant,
    burst: Option<Instant>,
}

#[derive(Default)]
pub struct Gate {
    cards: HashMap<String, Card>,
}

/// Whether a DRM card on this PCI device has a connector that is lit.
fn drives_display(device: &str) -> bool {
    for name in sys::list_dir(format!("{device}/drm")) {
        if !name.starts_with("card") {
            continue;
        }
        for entry in sys::list_dir(format!("{device}/drm/{name}")) {
            if entry.starts_with(&format!("{name}-"))
                && sys::read_text(format!("{device}/drm/{name}/{entry}/enabled")).as_deref() == Some("enabled")
            {
                return true;
            }
        }
    }
    false
}

pub fn power_of(device: &str) -> Power {
    if sys::read_text(format!("{device}/power/control")).as_deref() != Some("auto") {
        return Power::Free;
    }
    match sys::read_text(format!("{device}/power/runtime_status")).as_deref() {
        Some("suspended") | Some("suspending") => Power::Asleep,
        Some("active") | Some("resuming") => {
            if drives_display(device) {
                return Power::Free;
            }
            let ms = sys::read_u64(format!("{device}/power/autosuspend_delay_ms")).unwrap_or(5000);
            Power::Idleable(Duration::from_millis(ms))
        }
        _ => Power::Free,
    }
}

/// Decide a read at `now` for a card whose last burst opened at `burst`:
/// (may read, opens a new burst).
pub fn decide(burst: Option<Instant>, delay: Duration, now: Instant) -> (bool, bool) {
    match burst.map(|at| now.saturating_duration_since(at)) {
        Some(age) if age < BURST => (true, false),
        Some(age) if age < delay + MARGIN => (false, false),
        _ => (true, true),
    }
}

impl Gate {
    fn card(&mut self, device: &str, now: Instant) -> &mut Card {
        let card = self.cards.entry(device.to_string()).or_insert_with(|| Card {
            power: power_of(device),
            checked: now,
            burst: None,
        });
        if now.saturating_duration_since(card.checked) >= RECHECK {
            card.power = power_of(device);
            card.checked = now;
        }
        card
    }

    pub fn asleep(&mut self, device: &str) -> bool {
        self.card(device, Instant::now()).power == Power::Asleep
    }

    /// Whether the hardware of `device` (a canonical PCI path) may be read now.
    pub fn may_read(&mut self, device: &str) -> bool {
        let now = Instant::now();
        let card = self.card(device, now);
        match card.power {
            Power::Free => true,
            Power::Asleep => false,
            Power::Idleable(delay) => {
                let (ok, opens) = decide(card.burst, delay, now);
                if opens {
                    card.burst = Some(now);
                }
                ok
            }
        }
    }
}

/// The canonical PCI device behind a sysfs path, when it is a graphics card
/// (it has a drm folder).
pub fn gpu_device_of(path: &Path) -> Option<String> {
    let dev = sys::canonical(path)?;
    let dev = dev.to_string_lossy().into_owned();
    sys::exists(format!("{dev}/drm")).then_some(dev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bursts_then_rests() {
        let t0 = Instant::now();
        let delay = Duration::from_secs(5);
        assert_eq!(decide(None, delay, t0), (true, true));
        assert_eq!(decide(Some(t0), delay, t0 + Duration::from_millis(200)), (true, false));
        assert_eq!(decide(Some(t0), delay, t0 + Duration::from_secs(2)), (false, false));
        assert_eq!(decide(Some(t0), delay, t0 + Duration::from_secs(7)), (true, true));
    }
}
