//! Auto connect and auto switch (ADR-0009): a pure decision engine.
//!
//! It is fed the latest probe results and answers *what to do*; it never touches
//! the system. One [`Autopilot`] serves one kind of setting (proxy or DNS), so
//! the two stay independent.
//!
//! * **Auto connect** connects when something answers and disconnects (restoring
//!   the original settings) when nothing does.
//! * **Auto switch** keeps an active connection on the fastest profile; it never
//!   connects or disconnects by itself.
//! * **Both** together: connect to the fastest, fail over, disconnect if all are dead.
//!
//! Two guards prevent flapping: a profile must fail several probes *in a row*
//! before it is given up on, and another must be clearly faster to win a switch.

use crate::connectivity::{check_range, SettingsError};

/// The outcome of probing one profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// It answered within the timeout.
    Alive {
        /// Round-trip time in milliseconds.
        latency_ms: u32,
    },
    /// It did not answer, or answered wrongly.
    Dead,
}

/// A probe result for one profile of the pool, in the user's priority order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// Profile id.
    pub id: String,
    /// What the probe found.
    pub health: Health,
}

impl Probe {
    /// A result for the profile `id`.
    pub fn new(id: impl Into<String>, health: Health) -> Self {
        Self {
            id: id.into(),
            health,
        }
    }
}

/// What the caller should do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Leave things as they are.
    Keep,
    /// Activate (or switch to) this profile.
    Activate(String),
    /// Restore the original settings.
    Deactivate,
}

/// Which automatic behaviours are on, and how cautious they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoSettings {
    connect: bool,
    switch: bool,
    failures_before_giving_up: u8,
    min_gain_ms: u32,
    min_gain_percent: u8,
}

const MAX_GAIN_PERCENT: u8 = 90;

impl AutoSettings {
    /// Defaults: give up after 3 failures in a row; switch only for a gain of
    /// at least 50 ms *and* 20 %.
    pub fn new(connect: bool, switch: bool) -> Self {
        Self {
            connect,
            switch,
            failures_before_giving_up: 3,
            min_gain_ms: 50,
            min_gain_percent: 20,
        }
    }

    /// How many probes in a row must fail before the active profile is given up on (at least 1).
    pub fn with_failures(mut self, failures: u8) -> Result<Self, SettingsError> {
        check_range("failures before giving up", failures.into(), 1, 255)?;
        self.failures_before_giving_up = failures;
        Ok(self)
    }

    /// How much faster another profile must be to win a switch (percent at most 90).
    pub fn with_min_gain(mut self, ms: u32, percent: u8) -> Result<Self, SettingsError> {
        check_range(
            "minimum gain (percent)",
            percent.into(),
            0,
            MAX_GAIN_PERCENT.into(),
        )?;
        self.min_gain_ms = ms;
        self.min_gain_percent = percent;
        Ok(self)
    }
}

/// Decides, probe round by probe round, when to connect, switch or disconnect.
#[derive(Debug, Clone)]
pub struct Autopilot {
    settings: AutoSettings,
    misses: u8,
}

impl Autopilot {
    /// An autopilot that has seen no failures yet.
    pub fn new(settings: AutoSettings) -> Self {
        Self {
            settings,
            misses: 0,
        }
    }

    /// Looks at one round of probes (`active` is the profile currently applied by
    /// Hopswitch, if any) and says what to do. A profile outside the pool is the
    /// user's own choice and is never touched.
    pub fn observe(&mut self, active: Option<&str>, probes: &[Probe]) -> Decision {
        let s = self.settings;
        if !s.connect && !s.switch {
            return Decision::Keep;
        }
        let fastest = fastest_alive(probes);

        let Some(active) = active else {
            self.misses = 0;
            if !s.connect {
                return Decision::Keep;
            }
            let pick = if s.switch {
                fastest
            } else {
                first_alive(probes)
            };
            return pick.map_or(Decision::Keep, |(p, _)| Decision::Activate(p.id.clone()));
        };
        let Some(current) = probes.iter().find(|p| p.id == active) else {
            self.misses = 0;
            return Decision::Keep;
        };

        match current.health {
            Health::Alive { latency_ms } => {
                self.misses = 0;
                match fastest {
                    Some((best, best_ms))
                        if s.switch
                            && best.id != active
                            && self.clearly_faster(best_ms, latency_ms) =>
                    {
                        Decision::Activate(best.id.clone())
                    }
                    _ => Decision::Keep,
                }
            }
            Health::Dead => {
                self.misses = self.misses.saturating_add(1);
                if self.misses < s.failures_before_giving_up {
                    return Decision::Keep;
                }
                if let (true, Some((best, _))) = (s.switch, fastest) {
                    self.misses = 0;
                    Decision::Activate(best.id.clone())
                } else if s.connect {
                    self.misses = 0;
                    Decision::Deactivate
                } else {
                    Decision::Keep
                }
            }
        }
    }

    fn clearly_faster(&self, best_ms: u32, current_ms: u32) -> bool {
        let gain = u64::from(current_ms.saturating_sub(best_ms));
        gain >= u64::from(self.settings.min_gain_ms)
            && gain * 100 >= u64::from(current_ms) * u64::from(self.settings.min_gain_percent)
    }
}

fn alive_latency(probe: &Probe) -> Option<u32> {
    match probe.health {
        Health::Alive { latency_ms } => Some(latency_ms),
        Health::Dead => None,
    }
}

fn first_alive(probes: &[Probe]) -> Option<(&Probe, u32)> {
    probes
        .iter()
        .find_map(|p| alive_latency(p).map(|ms| (p, ms)))
}

/// The live profile with the lowest latency; the earlier one wins a tie.
fn fastest_alive(probes: &[Probe]) -> Option<(&Probe, u32)> {
    probes
        .iter()
        .filter_map(|p| alive_latency(p).map(|ms| (p, ms)))
        .min_by_key(|(_, ms)| *ms)
}
