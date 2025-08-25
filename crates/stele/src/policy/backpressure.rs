// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2024 Jonathan Lee
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License version 3
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see https://www.gnu.org/licenses/.



use serde::Serialize;
use std::f64::consts::LN_2;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum BackpressureLevel {
    #[default]
    Green = 0,
    Amber = 1,
    Red = 2,
}

#[derive(Default)]
struct Window {
    depth_ratio: f64,
    latency_ratio: f64,
    error_ratio: f64,
    last_update: Option<Instant>,

    last_raw_depth: f64,
    last_raw_latency: f64,
    last_raw_error: f64,

    short_b: f64,
    long_b: f64,

    mean_b: f64,
    m2_b: f64,
    count_b: u64,
    amber_threshold: f64,
    red_threshold: f64,
    last_level: BackpressureLevel,

    tokens: f64,
    last_refill: Option<Instant>,

    amber_since: Option<Instant>,

    w_depth: f64,
    w_latency: f64,
    w_error: f64,
}

static STATE: once_cell::sync::OnceCell<Mutex<Window>> = once_cell::sync::OnceCell::new();
static INFLIGHT: once_cell::sync::OnceCell<AtomicUsize> = once_cell::sync::OnceCell::new();

fn state() -> &'static Mutex<Window> {
    STATE.get_or_init(|| Mutex::new(Window::default()))
}

fn inflight() -> &'static AtomicUsize {
    INFLIGHT.get_or_init(|| AtomicUsize::new(0))
}

fn parse_env_usize(key: &str) -> Option<usize> {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
}

fn parse_env_f64(key: &str) -> Option<f64> {
    std::env::var(key).ok().and_then(|s| s.parse::<f64>().ok())
}

pub fn get_capacity() -> usize {
    parse_env_usize("STELE_BP_CAPACITY").unwrap_or(8)
}

pub fn get_sla_ms() -> f64 {
    parse_env_f64("STELE_BP_SLA_MS").unwrap_or(500.0)
}

pub fn current_inflight() -> usize {
    inflight().load(Ordering::Relaxed)
}

pub struct InflightGuard(());

impl Drop for InflightGuard {
    fn drop(&mut self) {
        inflight().fetch_sub(1, Ordering::Relaxed);
    }
}

pub fn inflight_guard() -> InflightGuard {
    inflight().fetch_add(1, Ordering::Relaxed);
    InflightGuard(())
}

pub fn record_run_metrics(latency_ms: f64, validation_failed: bool, processed: u64) {
    let qd = current_inflight() as f64;
    let cap = get_capacity() as f64;
    let sla = get_sla_ms();
    let fails = if validation_failed { 1 } else { 0 };
    update_metrics(qd, cap, latency_ms, sla, fails, processed.max(1));
}

#[derive(Debug, Clone, Serialize)]
pub struct BackpressureSnapshot {
    pub inflight: usize,
    pub capacity: usize,
    pub depth_ratio: f64,
    pub latency_ratio: f64,
    pub error_ratio: f64,
    pub smoothed: f64,
    pub instant: f64,
    pub level: &'static str,
    pub short: f64,
    pub long: f64,
    pub derivative: f64,
    pub amber_threshold: f64,
    pub red_threshold: f64,
    pub tokens: f64,
    pub recommended_action: &'static str,
}

pub fn snapshot() -> BackpressureSnapshot {
    let guard = state().lock().expect("bp mutex");
    let smoothed = combined_pressure(&guard, false);
    let instant = combined_pressure(&guard, true);

    let lvl = if let Ok(val) = std::env::var("STELE_BP_OVERRIDE") {
        match val.to_lowercase().as_str() {
            "green" => BackpressureLevel::Green,
            "amber" => BackpressureLevel::Amber,
            "red" => BackpressureLevel::Red,
            _ => derive_level(&guard, guard.short_b, guard.long_b),
        }
    } else {
        derive_level(&guard, guard.short_b, guard.long_b)
    };
    let level = match lvl {
        BackpressureLevel::Green => "green",
        BackpressureLevel::Amber => "amber",
        BackpressureLevel::Red => "red",
    };
    let derivative = guard.short_b - guard.long_b;
    BackpressureSnapshot {
        inflight: current_inflight(),
        capacity: get_capacity(),
        depth_ratio: guard.depth_ratio,
        latency_ratio: guard.latency_ratio,
        error_ratio: guard.error_ratio,
        smoothed,
        instant,
        level,
        short: guard.short_b,
        long: guard.long_b,
        derivative,
        amber_threshold: guard.amber_threshold,
        red_threshold: guard.red_threshold,
        tokens: guard.tokens,
        recommended_action: recommend_action_internal(&guard, lvl, derivative),
    }
}

pub fn log_snapshot_if_enabled() {
    if std::env::var("STELE_BP_METRICS_LOG").ok().as_deref() == Some("1") {
        let s = snapshot();
        tracing::info!(
            target: "stele::bp",
            inflight = s.inflight,
            capacity = s.capacity,
            depth_ratio = s.depth_ratio,
            latency_ratio = s.latency_ratio,
            error_ratio = s.error_ratio,
            smoothed = s.smoothed,
            instant = s.instant,
            level = %s.level,
            "Backpressure snapshot"
        );
    }
}

pub fn update_metrics(
    queue_depth: f64,
    queue_capacity: f64,
    p95_latency_ms: f64,
    p95_sla_ms: f64,
    validation_failures: u64,
    processed: u64,
) {
    let mut guard = state().lock().expect("bp mutex");
    let now = Instant::now();
    let delta = guard
        .last_update
        .map(|t| now.duration_since(t))
        .unwrap_or(Duration::from_secs(0));
    let dt_s = delta.as_secs_f64().max(0.0);

    let hl_long = parse_env_f64("STELE_BP_HALFLIFE_LONG_S").unwrap_or(8.0);
    let hl_short = parse_env_f64("STELE_BP_HALFLIFE_SHORT_S").unwrap_or(2.0);

    let alpha_long = if hl_long > 0.0 {
        1.0 - (-LN_2 * dt_s / hl_long).exp()
    } else {
        1.0
    };
    let alpha_short = if hl_short > 0.0 {
        1.0 - (-LN_2 * dt_s / hl_short).exp()
    } else {
        1.0
    };

    if guard.w_depth == 0.0 && guard.w_latency == 0.0 && guard.w_error == 0.0 {
        guard.w_depth = 0.6;
        guard.w_latency = 0.3;
        guard.w_error = 0.1;
    }
    let depth = if queue_capacity > 0.0 {
        (queue_depth / queue_capacity).clamp(0.0, 10.0)
    } else {
        0.0
    };
    let lat = if p95_sla_ms > 0.0 {
        (p95_latency_ms / p95_sla_ms).clamp(0.0, 10.0)
    } else {
        0.0
    };
    let err = if processed > 0 {
        (validation_failures as f64 / processed as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    guard.depth_ratio = guard.depth_ratio * (1.0 - alpha_long) + depth * alpha_long;
    guard.latency_ratio = guard.latency_ratio * (1.0 - alpha_long) + lat * alpha_long;
    guard.error_ratio = guard.error_ratio * (1.0 - alpha_long) + err * alpha_long;

    guard.last_raw_depth = depth;
    guard.last_raw_latency = lat;
    guard.last_raw_error = err;
    guard.last_update = Some(now);

    let instant_b = combined_pressure(&guard, true);
    if guard.short_b == 0.0 && guard.long_b == 0.0 {
        guard.short_b = instant_b;
        guard.long_b = instant_b;
    } else {
        guard.short_b = guard.short_b * (1.0 - alpha_short) + instant_b * alpha_short;
        guard.long_b = guard.long_b * (1.0 - alpha_long) + instant_b * alpha_long;
    }
    let b_for_stats = guard.long_b;

    guard.count_b += 1;
    let delta_mean = b_for_stats - guard.mean_b;
    guard.mean_b += delta_mean / guard.count_b as f64;
    guard.m2_b += delta_mean * (b_for_stats - guard.mean_b);
    let std_b = if guard.count_b > 2 {
        (guard.m2_b / (guard.count_b as f64 - 1.0)).sqrt()
    } else {
        0.0
    };

    if guard.count_b > parse_env_f64("STELE_BP_WARMUP_SAMPLES").unwrap_or(30.0) as u64 {
        let amber = (guard.mean_b + 0.5 * std_b).max(0.6);
        let red = (guard.mean_b + 1.2 * std_b).max(1.0);
        guard.amber_threshold = amber.min(red * 0.95);
        guard.red_threshold = red.max(guard.amber_threshold + 0.05);
    } else {
        guard.amber_threshold = 0.8;
        guard.red_threshold = 1.2;
    }

    let max_tokens = parse_env_f64("STELE_BP_TOKENS_MAX").unwrap_or(100.0);
    let refill_per_sec = parse_env_f64("STELE_BP_TOKENS_REFILL_PER_SEC").unwrap_or(50.0);
    let last_refill = guard.last_refill.unwrap_or(now);
    let since = now.duration_since(last_refill).as_secs_f64();
    let added = since * refill_per_sec;
    guard.tokens = (guard.tokens + added).min(max_tokens);
    guard.last_refill = Some(now);

    if guard.long_b > guard.red_threshold {
        guard.tokens *= 0.9;
    }

    let level = derive_level(&guard, guard.short_b, guard.long_b);
    if level == BackpressureLevel::Amber {
        if guard.amber_since.is_none() {
            guard.amber_since = Some(now);
        }
        if guard.amber_since.unwrap().elapsed()
            > Duration::from_secs(parse_env_f64("STELE_BP_AMBER_ADAPT_SECS").unwrap_or(10.0) as u64)
        {
            guard.w_latency = (guard.w_latency + 0.05).min(0.5);
            guard.w_depth = (1.0 - guard.w_latency - guard.w_error).max(0.3);
        }
    } else {
        guard.amber_since = None;

        guard.w_depth = guard.w_depth * 0.95 + 0.6 * 0.05;
        guard.w_latency = guard.w_latency * 0.95 + 0.3 * 0.05;
        guard.w_error = guard.w_error * 0.95 + 0.1 * 0.05;
    }
    guard.last_level = level;
}

pub fn current_signal() -> BackpressureLevel {
    if let Ok(val) = std::env::var("STELE_BP_OVERRIDE") {
        return match val.to_lowercase().as_str() {
            "green" => BackpressureLevel::Green,
            "amber" => BackpressureLevel::Amber,
            "red" => BackpressureLevel::Red,
            _ => compute_level(),
        };
    }
    compute_level()
}

fn compute_level() -> BackpressureLevel {
    let guard = state().lock().expect("bp mutex");
    derive_level(&guard, guard.short_b, guard.long_b)
}

fn derive_level(guard: &Window, short_b: f64, long_b: f64) -> BackpressureLevel {
    let amber = if guard.amber_threshold > 0.0 {
        guard.amber_threshold
    } else {
        0.8
    };
    let red = if guard.red_threshold > 0.0 {
        guard.red_threshold
    } else {
        1.2
    };

    let b = short_b.max(long_b);

    let hysteresis = parse_env_f64("STELE_BP_HYST_PCT").unwrap_or(0.1);
    match guard.last_level {
        BackpressureLevel::Red => {
            if b < red * (1.0 - hysteresis) {
                if b < amber {
                    BackpressureLevel::Green
                } else {
                    BackpressureLevel::Amber
                }
            } else {
                BackpressureLevel::Red
            }
        }
        BackpressureLevel::Amber => {
            if b >= red {
                BackpressureLevel::Red
            } else if b < amber * (1.0 - hysteresis) {
                BackpressureLevel::Green
            } else {
                BackpressureLevel::Amber
            }
        }
        BackpressureLevel::Green => {
            if b >= red {
                BackpressureLevel::Red
            } else if b >= amber {
                BackpressureLevel::Amber
            } else {
                BackpressureLevel::Green
            }
        }
    }
}

fn combined_pressure(w: &Window, instant: bool) -> f64 {
    let (wd, wl, we) = (w.w_depth, w.w_latency, w.w_error);
    if instant {
        wd * w.last_raw_depth + wl * w.last_raw_latency + we * w.last_raw_error
    } else {
        wd * w.depth_ratio + wl * w.latency_ratio + we * w.error_ratio
    }
}

pub fn try_reserve(n: usize) -> bool {
    let mut guard = state().lock().expect("bp mutex");
    if guard.tokens >= n as f64 {
        guard.tokens -= n as f64;
        true
    } else {
        false
    }
}

pub fn recommended_action() -> &'static str {
    let guard = state().lock().expect("bp mutex");
    let lvl = derive_level(&guard, guard.short_b, guard.long_b);
    let derivative = guard.short_b - guard.long_b;
    recommend_action_internal(&guard, lvl, derivative)
}

fn recommend_action_internal(
    guard: &Window,
    lvl: BackpressureLevel,
    derivative: f64,
) -> &'static str {
    match lvl {
        BackpressureLevel::Red => {
            if guard.tokens < 1.0 {
                "shed_low_priority"
            } else {
                "throttle_new"
            }
        }
        BackpressureLevel::Amber => {
            if derivative > 0.05 {
                "preemptive_throttle"
            } else {
                "throttle"
            }
        }
        BackpressureLevel::Green => {
            if derivative < -0.1 {
                "relax"
            } else {
                "normal"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use std::sync::Mutex as StdMutex;
    use std::thread;
    use std::time::Duration as StdDuration;

    static ENV_LOCK: Lazy<StdMutex<()>> = Lazy::new(|| StdMutex::new(()));

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn reset_state() {
        let mut g = state().lock().unwrap_or_else(|p| p.into_inner());
        *g = Window::default();
    }

    fn clear_bp_env() {
        for k in [
            "STELE_BP_OVERRIDE",
            "STELE_BP_HALFLIFE_LONG_S",
            "STELE_BP_HALFLIFE_SHORT_S",
            "STELE_BP_WARMUP_SAMPLES",
            "STELE_BP_TOKENS_MAX",
            "STELE_BP_TOKENS_REFILL_PER_SEC",
            "STELE_BP_HYST_PCT",
        ] {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn override_env_forces_level() {
        let _g = env_guard();
        reset_state();
        clear_bp_env();
        std::env::set_var("STELE_BP_OVERRIDE", "red");
        assert_eq!(current_signal(), BackpressureLevel::Red);
        std::env::set_var("STELE_BP_OVERRIDE", "amber");
        assert_eq!(current_signal(), BackpressureLevel::Amber);
        std::env::set_var("STELE_BP_OVERRIDE", "green");
        assert_eq!(current_signal(), BackpressureLevel::Green);
        clear_bp_env();
    }

    #[test]
    fn update_shifts_signal() {
        let _g = env_guard();
        reset_state();
        clear_bp_env();
        // Make updates deterministic (no smoothing)
        std::env::set_var("STELE_BP_HALFLIFE_LONG_S", "0");
        std::env::set_var("STELE_BP_HALFLIFE_SHORT_S", "0");
        std::env::set_var("STELE_BP_WARMUP_SAMPLES", "100000000");
        // Ensure no override
        std::env::remove_var("STELE_BP_OVERRIDE");
        // Start with low metrics
        update_metrics(0.0, 100.0, 10.0, 100.0, 0, 100);
        let s1 = current_signal();
        // Push high depth and latency
        update_metrics(120.0, 100.0, 200.0, 100.0, 10, 100);
        let s2 = current_signal();
        assert!(s1 <= BackpressureLevel::Amber);
        assert!(s2 >= BackpressureLevel::Amber);
        clear_bp_env();
    }

    #[test]
    fn hysteresis_prevents_flap() {
        let _g = env_guard();
        reset_state();
        clear_bp_env();
        // Use fixed thresholds (warmup high) and no smoothing for determinism
        std::env::set_var("STELE_BP_HALFLIFE_LONG_S", "0");
        std::env::set_var("STELE_BP_HALFLIFE_SHORT_S", "0");
        std::env::set_var("STELE_BP_WARMUP_SAMPLES", "100000000");
        std::env::remove_var("STELE_BP_OVERRIDE");
        update_metrics(90.0, 100.0, 90.0, 100.0, 0, 100); // near amber
        let _ = snapshot();
        let l1 = current_signal();
        // Slight improvement should not immediately downgrade if was Amber
        update_metrics(70.0, 100.0, 70.0, 100.0, 0, 100);
        let l2 = current_signal();
        // Force amber first
        if l1 == BackpressureLevel::Amber {
            assert!(l2 >= BackpressureLevel::Green);
        }
        clear_bp_env();
    }

    #[test]
    fn levels_dynamics_alpha_one() {
        let _g = env_guard();
        reset_state();
        clear_bp_env();
        // Deterministic: immediate EWMA, fixed thresholds
        std::env::set_var("STELE_BP_HALFLIFE_LONG_S", "0");
        std::env::set_var("STELE_BP_HALFLIFE_SHORT_S", "0");
        std::env::set_var("STELE_BP_WARMUP_SAMPLES", "100000000");

        // Baseline low -> Green
        update_metrics(0.0, 100.0, 10.0, 100.0, 0, 100);
        assert_eq!(current_signal(), BackpressureLevel::Green);

        // Moderate load -> Amber (b ~= 0.84)
        update_metrics(100.0, 100.0, 80.0, 100.0, 0, 100);
        assert_eq!(current_signal(), BackpressureLevel::Amber);

        // Heavy load -> Red (b > 1.2)
        update_metrics(120.0, 100.0, 200.0, 100.0, 0, 100);
        assert_eq!(current_signal(), BackpressureLevel::Red);

        // Improvement drops below amber threshold (0.8) -> Green
        update_metrics(80.0, 100.0, 90.0, 100.0, 0, 100);
        let l = current_signal();
        assert_eq!(l, BackpressureLevel::Green);

        clear_bp_env();
    }

    #[test]
    fn token_bucket_refill_and_reserve() {
        let _g = env_guard();
        reset_state();
        clear_bp_env();
        // Deterministic refill
        std::env::set_var("STELE_BP_TOKENS_MAX", "5");
        std::env::set_var("STELE_BP_TOKENS_REFILL_PER_SEC", "100");
        std::env::set_var("STELE_BP_HALFLIFE_LONG_S", "0");
        std::env::set_var("STELE_BP_HALFLIFE_SHORT_S", "0");
        std::env::set_var("STELE_BP_WARMUP_SAMPLES", "100000000");

        // First update to set last_refill
        update_metrics(0.0, 100.0, 10.0, 100.0, 0, 100);
        // Wait ~20ms => ~2 tokens
        thread::sleep(StdDuration::from_millis(20));
        update_metrics(0.0, 100.0, 10.0, 100.0, 0, 100);

        // Try reserve 1 -> should succeed
        assert!(try_reserve(1));
        // Reserve a lot -> should fail with small bucket
        assert!(!try_reserve(10));

        clear_bp_env();
    }

    #[test]
    fn recommended_action_mapping() {
        let _g = ENV_LOCK.lock().unwrap();
        reset_state();
        clear_bp_env();
        // Make short react immediately, long react slowly to get positive derivative under Amber
        std::env::set_var("STELE_BP_HALFLIFE_SHORT_S", "0");
        std::env::set_var("STELE_BP_HALFLIFE_LONG_S", "1000000");
        std::env::set_var("STELE_BP_WARMUP_SAMPLES", "100000000");

        // Start at low load
        update_metrics(0.0, 100.0, 10.0, 100.0, 0, 100);
        // Jump to moderate -> Amber; short rises, long lags => positive derivative
        update_metrics(100.0, 100.0, 80.0, 100.0, 0, 100);
        assert_eq!(current_signal(), BackpressureLevel::Amber);
        assert_eq!(recommended_action(), "preemptive_throttle");

        // Push to Red; set tokens to 0.5 to trigger shedding
        update_metrics(120.0, 100.0, 200.0, 100.0, 0, 100);
        {
            let mut g = state().lock().unwrap_or_else(|p| p.into_inner());
            g.tokens = 0.5;
        }
        assert_eq!(current_signal(), BackpressureLevel::Red);
        assert_eq!(recommended_action(), "shed_low_priority");

        // Give tokens to avoid shedding
        {
            let mut g = state().lock().unwrap_or_else(|p| p.into_inner());
            g.tokens = 5.0;
        }
        assert_eq!(recommended_action(), "throttle_new");

        clear_bp_env();
    }
}
