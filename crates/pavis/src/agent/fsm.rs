use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pavis_core::ConfigVersion;

pub const WAIT_MS: u64 = 30_000;
pub const REJECT_TTL: Duration = Duration::from_secs(10 * 60);
const BACKOFF_BASE: Duration = Duration::from_millis(250);
const BACKOFF_CAP: Duration = Duration::from_millis(5_000);
const BACKOFF_JITTER_PCT: u64 = 10;

#[derive(Debug, Clone)]
pub struct Context {
    pub last_applied_etag: Option<String>,
    pub last_rejected_etag: Option<String>,
    pub last_rejected_until: Option<Instant>,
    pub backoff_attempt: u32,
    pub observed_version: Option<ConfigVersion>,
    /// After receiving 410, send this many unconditional requests before resuming conditional fetching
    pub force_unconditional_count: u32,
}

impl Context {
    pub fn new() -> Self {
        Self {
            last_applied_etag: None,
            last_rejected_etag: None,
            last_rejected_until: None,
            backoff_attempt: 0,
            observed_version: None,
            force_unconditional_count: 0,
        }
    }

    pub fn conditional_etag(&mut self, now: Instant) -> Option<String> {
        self.clear_rejected_if_expired(now);

        // If we're in forced unconditional mode (after 410), don't send ETag
        if self.force_unconditional_count > 0 {
            let before = self.force_unconditional_count;
            self.force_unconditional_count = self.force_unconditional_count.saturating_sub(1);
            let after = self.force_unconditional_count;
            tracing::debug!(
                force_unconditional_before = before,
                force_unconditional_after = after,
                "conditional_etag: forced unconditional mode active, returning None"
            );
            return None;
        }

        let result = if let Some(etag) = self.last_rejected_etag.as_deref() {
            Some(etag.to_string())
        } else {
            self.last_applied_etag.clone()
        };

        tracing::debug!(
            force_unconditional_count = self.force_unconditional_count,
            has_last_applied = self.last_applied_etag.is_some(),
            has_last_rejected = self.last_rejected_etag.is_some(),
            returning_etag = result.is_some(),
            "conditional_etag: normal mode"
        );

        result
    }

    fn clear_rejected_if_expired(&mut self, now: Instant) {
        if let Some(until) = self.last_rejected_until
            && now >= until
        {
            self.last_rejected_etag = None;
            self.last_rejected_until = None;
        }
    }

    fn set_rejected(&mut self, etag: String, now: Instant) {
        self.last_rejected_etag = Some(etag);
        self.last_rejected_until = Some(now + REJECT_TTL);
    }

    fn clear_conditional_state(&mut self) {
        tracing::info!(
            prev_last_applied_etag = ?self.last_applied_etag,
            prev_last_rejected_etag = ?self.last_rejected_etag,
            prev_force_unconditional_count = self.force_unconditional_count,
            "clear_conditional_state: resetting state after 410 Gone"
        );
        self.last_applied_etag = None;
        self.last_rejected_etag = None;
        self.last_rejected_until = None;
        // After 410, force at least 2 unconditional requests to ensure full resync
        self.force_unconditional_count = 2;
        tracing::info!(
            new_force_unconditional_count = self.force_unconditional_count,
            "clear_conditional_state: set force_unconditional_count=2"
        );
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum State {
    Idle,
    Fetching,
    Verifying(VerifyingData),
    Applying(VerifiedUpdate),
    BackoffSleeping,
    Stopped,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct VerifyingData {
    pub etag: String,
    pub version: Option<ConfigVersion>,
    pub size: Option<u64>,
    pub bytes: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct VerifiedUpdate {
    pub etag: String,
    pub version: Option<ConfigVersion>,
    pub size: Option<u64>,
    pub tmp_path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum Response {
    NewArtifact {
        etag: String,
        version: Option<ConfigVersion>,
        size: Option<u64>,
        bytes: Vec<u8>,
    },
    NoUpdate,
    NeedResync,
    TransientUnavailable,
}

#[derive(Debug, Clone)]
pub enum Event {
    Start {
        now: Instant,
    },
    Response {
        response: Response,
        now: Instant,
    },
    VerifyOk {
        update: VerifiedUpdate,
        now: Instant,
    },
    VerifyFail {
        etag: String,
        now: Instant,
    },
    ApplyOk {
        etag: String,
        version: Option<ConfigVersion>,
        now: Instant,
    },
    ApplyFail {
        etag: String,
        now: Instant,
    },
    TimerFired {
        now: Instant,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum Effect {
    FetchConditional {
        wait_ms: u64,
        etag: String,
    },
    FetchUnconditional {
        wait_ms: u64,
    },
    Verify {
        etag: String,
        version: Option<ConfigVersion>,
        size: Option<u64>,
        bytes: Vec<u8>,
    },
    Apply {
        update: VerifiedUpdate,
    },
    ScheduleTimer {
        duration: Duration,
    },
    DiscardTemp {
        path: PathBuf,
    },
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct StateSummary {
    pub state: &'static str,
    pub last_applied_etag: Option<String>,
    pub last_rejected_etag: Option<String>,
    pub backoff_attempt: u32,
    pub observed_version: Option<ConfigVersion>,
}

#[derive(Debug)]
pub struct Fsm {
    state: State,
    ctx: Context,
}

impl Fsm {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            ctx: Context::new(),
        }
    }

    pub fn new_with_lkg_path(_local_lkg_path: PathBuf) -> Self {
        Self {
            state: State::Idle,
            ctx: Context::new(),
        }
    }

    #[allow(dead_code)]
    pub fn current_state(&self) -> StateSummary {
        StateSummary {
            state: match self.state {
                State::Idle => "Idle",
                State::Fetching => "Fetching",
                State::Verifying(_) => "Verifying",
                State::Applying(_) => "Applying",
                State::BackoffSleeping => "BackoffSleeping",
                State::Stopped => "Stopped",
            },
            last_applied_etag: self.ctx.last_applied_etag.clone(),
            last_rejected_etag: self.ctx.last_rejected_etag.clone(),
            backoff_attempt: self.ctx.backoff_attempt,
            observed_version: self.ctx.observed_version,
        }
    }

    pub fn context(&self) -> &Context {
        &self.ctx
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.ctx
    }

    pub fn tick(&mut self, event: Event) -> Vec<Effect> {
        let mut effects = Vec::new();
        match (&mut self.state, event) {
            (State::Idle, Event::Start { now }) => {
                self.ctx.clear_rejected_if_expired(now);
                self.state = State::Fetching;
                effects.push(Effect::FetchUnconditional { wait_ms: WAIT_MS });
            }
            (State::Idle, Event::TimerFired { now }) => {
                self.ctx.clear_rejected_if_expired(now);
                self.state = State::Fetching;
                if let Some(etag) = self.ctx.conditional_etag(now) {
                    effects.push(Effect::FetchConditional {
                        wait_ms: WAIT_MS,
                        etag,
                    });
                } else {
                    effects.push(Effect::FetchUnconditional { wait_ms: WAIT_MS });
                }
            }
            (State::Idle, Event::Response { .. }) => {}
            (State::Idle, Event::VerifyOk { .. }) => {}
            (State::Idle, Event::VerifyFail { .. }) => {}
            (State::Idle, Event::ApplyOk { .. }) => {}
            (State::Idle, Event::ApplyFail { .. }) => {}
            (State::Idle, Event::Shutdown) => {
                self.state = State::Stopped;
            }
            (State::Stopped, _) => {}
            (State::Fetching, Event::Response { response, now }) => {
                self.ctx.clear_rejected_if_expired(now);
                match response {
                    Response::NewArtifact {
                        etag,
                        version,
                        size,
                        bytes,
                    } => {
                        if self
                            .ctx
                            .last_rejected_etag
                            .as_deref()
                            .is_some_and(|rejected| rejected == etag)
                            && self
                                .ctx
                                .last_rejected_until
                                .is_some_and(|until| now < until)
                        {
                            self.state = State::Fetching;
                            self.ctx.backoff_attempt = 0;
                            if let Some(etag) = self.ctx.conditional_etag(now) {
                                effects.push(Effect::FetchConditional {
                                    wait_ms: WAIT_MS,
                                    etag,
                                });
                            } else {
                                effects.push(Effect::FetchUnconditional { wait_ms: WAIT_MS });
                            }
                        } else {
                            let verify_effect = Effect::Verify {
                                etag: etag.clone(),
                                version,
                                size,
                                bytes: bytes.clone(),
                            };
                            self.state = State::Verifying(VerifyingData {
                                etag,
                                version,
                                size,
                                bytes,
                            });
                            effects.push(verify_effect);
                        }
                    }
                    Response::NoUpdate => {
                        self.state = State::Fetching;
                        self.ctx.backoff_attempt = 0;
                        if let Some(etag) = self.ctx.conditional_etag(now) {
                            effects.push(Effect::FetchConditional {
                                wait_ms: WAIT_MS,
                                etag,
                            });
                        } else {
                            effects.push(Effect::FetchUnconditional { wait_ms: WAIT_MS });
                        }
                    }
                    Response::NeedResync => {
                        tracing::info!(
                            "fsm: received NeedResync (410 Gone), clearing conditional state"
                        );
                        self.ctx.clear_conditional_state();
                        self.ctx.backoff_attempt = 0;
                        self.state = State::Fetching;
                        tracing::info!(
                            force_unconditional_count = self.ctx.force_unconditional_count,
                            "fsm: pushing FetchUnconditional effect after NeedResync"
                        );
                        effects.push(Effect::FetchUnconditional { wait_ms: WAIT_MS });
                    }
                    Response::TransientUnavailable => {
                        let delay = backoff_delay(self.ctx.backoff_attempt);
                        self.ctx.backoff_attempt = self.ctx.backoff_attempt.saturating_add(1);
                        self.state = State::BackoffSleeping;
                        effects.push(Effect::ScheduleTimer { duration: delay });
                    }
                }
            }
            (State::Fetching, Event::Shutdown) => {
                self.state = State::Stopped;
            }
            (State::Fetching, _) => {}
            (State::Verifying(_), Event::VerifyOk { update, now }) => {
                self.ctx.clear_rejected_if_expired(now);
                if self.ctx.last_applied_etag.as_deref() == Some(update.etag.as_str()) {
                    self.state = State::Fetching;
                    self.ctx.backoff_attempt = 0;
                    effects.push(Effect::DiscardTemp {
                        path: update.tmp_path,
                    });
                    if let Some(etag) = self.ctx.conditional_etag(now) {
                        effects.push(Effect::FetchConditional {
                            wait_ms: WAIT_MS,
                            etag,
                        });
                    } else {
                        effects.push(Effect::FetchUnconditional { wait_ms: WAIT_MS });
                    }
                } else {
                    self.state = State::Applying(update.clone());
                    effects.push(Effect::Apply { update });
                }
            }
            (State::Verifying(_), Event::VerifyFail { etag, now }) => {
                self.ctx.set_rejected(etag, now);
                let delay = backoff_delay(self.ctx.backoff_attempt);
                self.ctx.backoff_attempt = self.ctx.backoff_attempt.saturating_add(1);
                self.state = State::BackoffSleeping;
                effects.push(Effect::ScheduleTimer { duration: delay });
            }
            (State::Verifying(_), Event::Shutdown) => {
                self.state = State::Stopped;
            }
            (State::Verifying(_), _) => {}
            (State::Applying(_), Event::ApplyOk { etag, version, now }) => {
                // Only save the ETag if we're not in forced unconditional mode
                // (after 410, we want to continue unconditional fetching for a few rounds)
                if self.ctx.force_unconditional_count == 0 {
                    self.ctx.last_applied_etag = Some(etag);
                }
                self.ctx.last_rejected_etag = None;
                self.ctx.last_rejected_until = None;
                self.ctx.backoff_attempt = 0;
                self.ctx.observed_version = version;
                self.state = State::Fetching;
                if let Some(etag) = self.ctx.conditional_etag(now) {
                    effects.push(Effect::FetchConditional {
                        wait_ms: WAIT_MS,
                        etag,
                    });
                } else {
                    effects.push(Effect::FetchUnconditional { wait_ms: WAIT_MS });
                }
            }
            (State::Applying(_), Event::ApplyFail { etag, now }) => {
                self.ctx.set_rejected(etag, now);
                let delay = backoff_delay(self.ctx.backoff_attempt);
                self.ctx.backoff_attempt = self.ctx.backoff_attempt.saturating_add(1);
                self.state = State::BackoffSleeping;
                effects.push(Effect::ScheduleTimer { duration: delay });
            }
            (State::Applying(_), Event::Shutdown) => {
                self.state = State::Stopped;
            }
            (State::Applying(_), _) => {}
            (State::BackoffSleeping, Event::TimerFired { now }) => {
                self.ctx.clear_rejected_if_expired(now);
                self.state = State::Fetching;
                if let Some(etag) = self.ctx.conditional_etag(now) {
                    effects.push(Effect::FetchConditional {
                        wait_ms: WAIT_MS,
                        etag,
                    });
                } else {
                    effects.push(Effect::FetchUnconditional { wait_ms: WAIT_MS });
                }
            }
            (State::BackoffSleeping, Event::Shutdown) => {
                self.state = State::Stopped;
            }
            (State::BackoffSleeping, _) => {}
        }
        effects
    }

    #[allow(dead_code)]
    pub fn drain_effects(&mut self, events: VecDeque<Event>) -> Vec<Effect> {
        let mut all = Vec::new();
        for event in events {
            all.extend(self.tick(event));
        }
        all
    }
}

fn backoff_delay(attempt: u32) -> Duration {
    let factor = 1u32.checked_shl(attempt.min(10)).unwrap_or(u32::MAX);
    let exp = BACKOFF_BASE.saturating_mul(factor);
    let capped = if exp > BACKOFF_CAP { BACKOFF_CAP } else { exp };
    let jitter_range = capped.as_millis() * BACKOFF_JITTER_PCT as u128 / 100;
    if jitter_range == 0 {
        return capped;
    }
    let offset = rand::random::<u128>() % (2 * jitter_range + 1);
    let signed = offset as i128 - jitter_range as i128;
    if signed >= 0 {
        capped + Duration::from_millis(signed as u64)
    } else {
        capped - Duration::from_millis((-signed) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn idle_start_triggers_unconditional_fetch() {
        let mut fsm = Fsm::new();
        let effects = fsm.tick(Event::Start {
            now: Instant::now(),
        });
        assert!(
            matches!(effects.as_slice(), [Effect::FetchUnconditional { wait_ms }] if *wait_ms == WAIT_MS)
        );
    }

    #[test]
    fn no_update_keeps_long_polling() {
        let mut fsm = Fsm::new();
        fsm.tick(Event::Start {
            now: Instant::now(),
        });
        let effects = fsm.tick(Event::Response {
            response: Response::NoUpdate,
            now: Instant::now(),
        });
        assert!(
            matches!(effects.as_slice(), [Effect::FetchUnconditional { wait_ms }] if *wait_ms == WAIT_MS)
        );
    }

    #[test]
    fn need_resync_clears_state_and_fetches_unconditional() {
        let mut fsm = Fsm::new();
        fsm.context_mut().last_applied_etag = Some("sha256:abc".to_string());
        fsm.context_mut().last_rejected_etag = Some("sha256:def".to_string());
        fsm.state = State::Fetching;
        let effects = fsm.tick(Event::Response {
            response: Response::NeedResync,
            now: Instant::now(),
        });
        assert!(fsm.context().last_applied_etag.is_none());
        assert!(fsm.context().last_rejected_etag.is_none());
        assert!(
            matches!(effects.as_slice(), [Effect::FetchUnconditional { wait_ms }] if *wait_ms == WAIT_MS)
        );
    }

    #[test]
    fn verify_ok_dedup_skips_apply() {
        let mut fsm = Fsm::new();
        let now = Instant::now();
        fsm.context_mut().last_applied_etag = Some("sha256:same".to_string());
        fsm.state = State::Verifying(VerifyingData {
            etag: "sha256:same".to_string(),
            version: None,
            size: None,
            bytes: vec![1, 2, 3],
        });
        let effects = fsm.tick(Event::VerifyOk {
            update: VerifiedUpdate {
                etag: "sha256:same".to_string(),
                version: None,
                size: None,
                tmp_path: PathBuf::from("/tmp/test.pvs"),
            },
            now,
        });
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::DiscardTemp { .. }))
        );
    }

    #[test]
    fn transient_unavailable_enters_backoff() {
        let mut fsm = Fsm::new();
        fsm.state = State::Fetching;
        let effects = fsm.tick(Event::Response {
            response: Response::TransientUnavailable,
            now: Instant::now(),
        });
        assert!(matches!(fsm.state, State::BackoffSleeping));
        assert!(matches!(effects.as_slice(), [Effect::ScheduleTimer { .. }]));
    }

    #[test]
    fn rejected_etag_skip_avoids_verify() {
        let mut fsm = Fsm::new();
        let now = Instant::now();
        fsm.context_mut().last_rejected_etag = Some("sha256:bad".to_string());
        fsm.context_mut().last_rejected_until = Some(now + REJECT_TTL);
        fsm.state = State::Fetching;
        let effects = fsm.tick(Event::Response {
            response: Response::NewArtifact {
                etag: "sha256:bad".to_string(),
                version: None,
                size: None,
                bytes: vec![1, 2, 3],
            },
            now,
        });
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, Effect::Verify { .. }))
        );
        assert!(matches!(fsm.state, State::Fetching));
    }

    #[test]
    fn rejected_etag_ttl_expiry_clears_skip() {
        let mut ctx = Context::new();
        let now = Instant::now();
        ctx.last_applied_etag = Some("sha256:good".to_string());
        ctx.last_rejected_etag = Some("sha256:bad".to_string());
        ctx.last_rejected_until = Some(now - Duration::from_secs(1));
        let conditional = ctx.conditional_etag(now);
        assert_eq!(conditional.as_deref(), Some("sha256:good"));
        assert!(ctx.last_rejected_etag.is_none());
        assert!(ctx.last_rejected_until.is_none());
    }

    #[test]
    fn verify_fail_sets_rejected_and_fetches() {
        let mut fsm = Fsm::new();
        let now = Instant::now();
        fsm.state = State::Verifying(VerifyingData {
            etag: "sha256:bad".to_string(),
            version: None,
            size: None,
            bytes: vec![1],
        });
        let effects = fsm.tick(Event::VerifyFail {
            etag: "sha256:bad".to_string(),
            now,
        });
        assert_eq!(
            fsm.context().last_rejected_etag.as_deref(),
            Some("sha256:bad")
        );
        assert!(matches!(fsm.state, State::BackoffSleeping));
        assert!(matches!(effects.as_slice(), [Effect::ScheduleTimer { .. }]));
    }

    #[test]
    fn apply_fail_sets_rejected_and_fetches() {
        let mut fsm = Fsm::new();
        let now = Instant::now();
        fsm.state = State::Applying(VerifiedUpdate {
            etag: "sha256:bad".to_string(),
            version: None,
            size: None,
            tmp_path: PathBuf::from("/tmp/test.pvs"),
        });
        let effects = fsm.tick(Event::ApplyFail {
            etag: "sha256:bad".to_string(),
            now,
        });
        assert_eq!(
            fsm.context().last_rejected_etag.as_deref(),
            Some("sha256:bad")
        );
        assert!(matches!(fsm.state, State::BackoffSleeping));
        assert!(matches!(effects.as_slice(), [Effect::ScheduleTimer { .. }]));
    }

    #[test]
    fn need_resync_resets_backoff() {
        let mut fsm = Fsm::new();
        fsm.context_mut().backoff_attempt = 5;
        fsm.state = State::Fetching;
        let _ = fsm.tick(Event::Response {
            response: Response::NeedResync,
            now: Instant::now(),
        });
        assert_eq!(fsm.context().backoff_attempt, 0);
    }

    #[test]
    fn backoff_delay_is_capped_with_jitter() {
        let mut fsm = Fsm::new();
        fsm.context_mut().backoff_attempt = 20;
        fsm.state = State::Fetching;
        let effects = fsm.tick(Event::Response {
            response: Response::TransientUnavailable,
            now: Instant::now(),
        });
        let duration = match effects.as_slice() {
            [Effect::ScheduleTimer { duration }] => *duration,
            _ => panic!("expected ScheduleTimer effect"),
        };
        let jitter_range = BACKOFF_CAP.as_millis() * BACKOFF_JITTER_PCT as u128 / 100;
        let min = BACKOFF_CAP.saturating_sub(Duration::from_millis(jitter_range as u64));
        let max = BACKOFF_CAP + Duration::from_millis(jitter_range as u64);
        assert!(duration >= min);
        assert!(duration <= max);
    }

    #[test]
    fn shutdown_from_idle_transitions_to_stopped() {
        let mut fsm = Fsm::new();
        fsm.tick(Event::Shutdown);
        assert!(matches!(fsm.state, State::Stopped));
    }
}
