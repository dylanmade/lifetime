//! Time aggregation over observations.
//!
//! Each observation contributes time from its `recorded_at` to either the
//! next observation's `recorded_at` (for transitions) or the caller-supplied
//! `until` (for the most recent observation). Per-observation contributions
//! are capped at `max_gap` so a long sleep or crash doesn't get counted as
//! continuous app usage.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::model::{Observation, ObservationKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppDuration {
    pub app_name: String,
    pub bundle_id: Option<String>,
    pub active_seconds: u64,
    pub idle_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HourActivity {
    pub hour: u8,
    pub active_seconds: u64,
}

/// A contiguous span of time the user was in one app at one activity state.
/// Produced by [`aggregate_into_segments`] for timeline-style visualizations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSegment {
    pub app_name: String,
    pub bundle_id: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub starts_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub ends_at: OffsetDateTime,
    pub is_active: bool,
    /// Id of the first observation in this run. Stable across re-derivation
    /// (observations are append-only), so it anchors the identity of the
    /// derived auto-activity that annotations attach to.
    pub origin_observation_id: Uuid,
    /// Window title from the run's first observation, for activity detail.
    pub window_title: Option<String>,
}

pub fn aggregate_by_app(
    observations: &[Observation],
    until: OffsetDateTime,
    max_gap: Duration,
) -> Vec<AppDuration> {
    let mut sorted: Vec<&Observation> = observations.iter().collect();
    sorted.sort_by_key(|o| o.recorded_at);

    let mut totals: HashMap<String, AppDuration> = HashMap::new();

    for (i, obs) in sorted.iter().enumerate() {
        let next_time = if i + 1 < sorted.len() {
            sorted[i + 1].recorded_at
        } else {
            until
        };

        let mut duration = next_time - obs.recorded_at;
        if duration > max_gap {
            duration = max_gap;
        }
        if duration <= Duration::ZERO {
            continue;
        }
        let seconds = duration.whole_seconds() as u64;

        if let ObservationKind::AppUsage(sample) = &obs.kind {
            let entry = totals
                .entry(sample.app_name.clone())
                .or_insert_with(|| AppDuration {
                    app_name: sample.app_name.clone(),
                    bundle_id: sample.bundle_id.clone(),
                    active_seconds: 0,
                    idle_seconds: 0,
                });
            if sample.is_active {
                entry.active_seconds += seconds;
            } else {
                entry.idle_seconds += seconds;
            }
        }
    }

    let mut result: Vec<_> = totals.into_values().collect();
    result.sort_by(|a, b| b.active_seconds.cmp(&a.active_seconds));
    result
}

/// Bucket active seconds into the 24 hours of the day defined by
/// `[day_start, day_end)`. Each observation's effective span is clipped to
/// the day window and to `max_gap`, then split across hour boundaries.
/// Idle observations contribute zero seconds.
pub fn aggregate_by_hour(
    observations: &[Observation],
    day_start: OffsetDateTime,
    day_end: OffsetDateTime,
    max_gap: Duration,
) -> Vec<HourActivity> {
    let mut buckets = [0u64; 24];

    let mut sorted: Vec<&Observation> = observations.iter().collect();
    sorted.sort_by_key(|o| o.recorded_at);

    for (i, obs) in sorted.iter().enumerate() {
        let ObservationKind::AppUsage(sample) = &obs.kind else {
            continue;
        };
        if !sample.is_active {
            continue;
        }

        let raw_next = if i + 1 < sorted.len() {
            sorted[i + 1].recorded_at
        } else {
            day_end
        };

        let mut start = obs.recorded_at;
        let mut end = raw_next;
        if (end - start) > max_gap {
            end = start + max_gap;
        }
        if start < day_start {
            start = day_start;
        }
        if end > day_end {
            end = day_end;
        }
        if start >= end {
            continue;
        }

        while start < end {
            let offset = (start - day_start).whole_seconds();
            if offset < 0 {
                break;
            }
            let hour_index = (offset / 3600) as usize;
            if hour_index >= 24 {
                break;
            }
            let hour_end = day_start + Duration::seconds(((hour_index + 1) * 3600) as i64);
            let segment_end = if end < hour_end { end } else { hour_end };
            buckets[hour_index] += (segment_end - start).whole_seconds() as u64;
            start = segment_end;
        }
    }

    (0..24)
        .map(|h| HourActivity {
            hour: h as u8,
            active_seconds: buckets[h as usize],
        })
        .collect()
}

/// Walk observations and produce contiguous `AppSegment`s clipped to
/// `[day_start, day_end)`. Each app-usage observation contributes a span up to
/// the next observation, capped at `max_gap`. Adjacent spans with the same
/// `(app_name, is_active)` are merged when they meet exactly. Non-app-usage
/// observations are skipped (they implicitly terminate the previous segment
/// via their `recorded_at` becoming the next boundary).
pub fn aggregate_into_segments(
    observations: &[Observation],
    day_start: OffsetDateTime,
    day_end: OffsetDateTime,
    max_gap: Duration,
) -> Vec<AppSegment> {
    let mut sorted: Vec<&Observation> = observations.iter().collect();
    sorted.sort_by_key(|o| o.recorded_at);

    let mut segments: Vec<AppSegment> = Vec::new();

    for (i, obs) in sorted.iter().enumerate() {
        let ObservationKind::AppUsage(sample) = &obs.kind else {
            continue;
        };

        let raw_next = if i + 1 < sorted.len() {
            sorted[i + 1].recorded_at
        } else {
            day_end
        };

        let mut span_start = obs.recorded_at;
        let mut span_end = raw_next;
        if span_end - span_start > max_gap {
            span_end = span_start + max_gap;
        }
        if span_start < day_start {
            span_start = day_start;
        }
        if span_end > day_end {
            span_end = day_end;
        }
        if span_end <= span_start {
            continue;
        }

        if let Some(last) = segments.last_mut() {
            if last.app_name == sample.app_name
                && last.is_active == sample.is_active
                && last.ends_at == span_start
            {
                // Extend the run; keep the first observation's id/title as the
                // stable anchor.
                last.ends_at = span_end;
                continue;
            }
        }
        segments.push(AppSegment {
            app_name: sample.app_name.clone(),
            bundle_id: sample.bundle_id.clone(),
            starts_at: span_start,
            ends_at: span_end,
            is_active: sample.is_active,
            origin_observation_id: obs.id,
            window_title: sample.window_title.clone(),
        });
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AppUsageSample, ObservationKind};
    use time::macros::datetime;
    use uuid::Uuid;

    fn app_obs(at: OffsetDateTime, name: &str, is_active: bool) -> Observation {
        Observation {
            id: Uuid::new_v4(),
            device_id: Uuid::nil(),
            recorded_at: at,
            kind: ObservationKind::AppUsage(AppUsageSample {
                bundle_id: Some(format!("com.example.{name}")),
                app_name: name.to_string(),
                window_title: None,
                is_active,
            }),
        }
    }

    #[test]
    fn aggregates_session_with_one_switch() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 12:00 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 12:05 UTC), "Code", true),
            app_obs(datetime!(2026-05-27 12:08 UTC), "Safari", true),
        ];
        let until = datetime!(2026-05-27 12:10 UTC);
        let result = aggregate_by_app(&observations, until, Duration::minutes(10));

        let safari = result.iter().find(|d| d.app_name == "Safari").unwrap();
        let code = result.iter().find(|d| d.app_name == "Code").unwrap();
        // Safari: 12:00–12:05 (5min) + 12:08–12:10 (2min) = 7min
        assert_eq!(safari.active_seconds, 7 * 60);
        // Code: 12:05–12:08 = 3min
        assert_eq!(code.active_seconds, 3 * 60);
    }

    #[test]
    fn caps_long_gaps_at_max_gap() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 12:00 UTC), "Safari", true),
            // 2-hour gap (laptop slept)
            app_obs(datetime!(2026-05-27 14:00 UTC), "Code", true),
        ];
        let until = datetime!(2026-05-27 14:30 UTC);
        let result = aggregate_by_app(&observations, until, Duration::minutes(2));

        let safari = result.iter().find(|d| d.app_name == "Safari").unwrap();
        let code = result.iter().find(|d| d.app_name == "Code").unwrap();
        // Safari's gap to next observation is 2 hours → clipped to 2 min
        assert_eq!(safari.active_seconds, 2 * 60);
        // Code's gap to until is 30 min → clipped to 2 min
        assert_eq!(code.active_seconds, 2 * 60);
    }

    #[test]
    fn idle_observations_count_toward_idle_seconds() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 12:00 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 12:01 UTC), "Safari", false),
            app_obs(datetime!(2026-05-27 12:03 UTC), "Safari", true),
        ];
        let until = datetime!(2026-05-27 12:04 UTC);
        let result = aggregate_by_app(&observations, until, Duration::minutes(5));

        let safari = result.iter().find(|d| d.app_name == "Safari").unwrap();
        // Active: 12:00–12:01 (1min) + 12:03–12:04 (1min) = 2min
        // Idle: 12:01–12:03 (2min)
        assert_eq!(safari.active_seconds, 2 * 60);
        assert_eq!(safari.idle_seconds, 2 * 60);
    }

    #[test]
    fn empty_input_yields_empty_result() {
        let result = aggregate_by_app(
            &[],
            datetime!(2026-05-27 12:00 UTC),
            Duration::minutes(2),
        );
        assert!(result.is_empty());
    }

    #[test]
    fn unsorted_input_is_handled() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 12:08 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 12:00 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 12:05 UTC), "Code", true),
        ];
        let until = datetime!(2026-05-27 12:10 UTC);
        let result = aggregate_by_app(&observations, until, Duration::minutes(10));

        let safari = result.iter().find(|d| d.app_name == "Safari").unwrap();
        let code = result.iter().find(|d| d.app_name == "Code").unwrap();
        assert_eq!(safari.active_seconds, 5 * 60 + 2 * 60);
        assert_eq!(code.active_seconds, 3 * 60);
    }

    fn day_start() -> OffsetDateTime {
        datetime!(2026-05-27 00:00 UTC)
    }
    fn day_end() -> OffsetDateTime {
        datetime!(2026-05-28 00:00 UTC)
    }

    #[test]
    fn hourly_single_hour_span() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 09:10 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 09:25 UTC), "Code", true),
            // Idle terminator bounds the previous Code observation.
            app_obs(datetime!(2026-05-27 09:40 UTC), "Code", false),
        ];
        let result = aggregate_by_hour(
            &observations,
            day_start(),
            day_end(),
            Duration::minutes(30),
        );
        // 9:10–9:25 = 15min Safari (hour 9)
        // 9:25–9:40 = 15min Code (hour 9)
        // 9:40 idle, contributes 0
        assert_eq!(result[9].active_seconds, 30 * 60);
        assert_eq!(result[8].active_seconds, 0);
        assert_eq!(result[10].active_seconds, 0);
    }

    #[test]
    fn hourly_splits_across_hour_boundary() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 10:50 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 11:20 UTC), "Code", true),
            // Idle terminator bounds the previous Code observation.
            app_obs(datetime!(2026-05-27 11:30 UTC), "Code", false),
        ];
        let result = aggregate_by_hour(
            &observations,
            day_start(),
            day_end(),
            Duration::minutes(60),
        );
        // 10:50–11:00 = 10min Safari (hour 10)
        // 11:00–11:20 = 20min Safari (hour 11)
        // 11:20–11:30 = 10min Code (hour 11)
        assert_eq!(result[10].active_seconds, 10 * 60);
        assert_eq!(result[11].active_seconds, 30 * 60);
    }

    #[test]
    fn hourly_excludes_idle_observations() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 14:00 UTC), "Safari", false),
            app_obs(datetime!(2026-05-27 14:30 UTC), "Safari", true),
        ];
        let result = aggregate_by_hour(
            &observations,
            day_start(),
            day_end(),
            Duration::minutes(60),
        );
        // 14:00–14:30 idle → not counted
        // 14:30–day_end clipped to max_gap (60min), so 14:30–15:30 = 30min in 14, 30min in 15
        assert_eq!(result[14].active_seconds, 30 * 60);
        assert_eq!(result[15].active_seconds, 30 * 60);
    }

    #[test]
    fn hourly_empty_input_yields_24_zero_buckets() {
        let result = aggregate_by_hour(&[], day_start(), day_end(), Duration::minutes(30));
        assert_eq!(result.len(), 24);
        assert!(result.iter().all(|b| b.active_seconds == 0));
        for (i, b) in result.iter().enumerate() {
            assert_eq!(b.hour, i as u8);
        }
    }

    #[test]
    fn segments_merges_consecutive_same_app_observations() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 09:00 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 09:01 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 09:02 UTC), "Safari", true),
        ];
        let segments = aggregate_into_segments(
            &observations,
            day_start(),
            datetime!(2026-05-27 09:03 UTC),
            Duration::minutes(2),
        );
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].app_name, "Safari");
        assert_eq!(segments[0].starts_at, datetime!(2026-05-27 09:00 UTC));
        assert_eq!(segments[0].ends_at, datetime!(2026-05-27 09:03 UTC));
    }

    #[test]
    fn segments_splits_on_app_switch() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 09:00 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 09:05 UTC), "Code", true),
            app_obs(datetime!(2026-05-27 09:08 UTC), "Safari", true),
        ];
        let segments = aggregate_into_segments(
            &observations,
            day_start(),
            datetime!(2026-05-27 09:10 UTC),
            Duration::minutes(10),
        );
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].app_name, "Safari");
        assert_eq!(segments[0].ends_at, datetime!(2026-05-27 09:05 UTC));
        assert_eq!(segments[1].app_name, "Code");
        assert_eq!(segments[1].ends_at, datetime!(2026-05-27 09:08 UTC));
        assert_eq!(segments[2].app_name, "Safari");
        assert_eq!(segments[2].ends_at, datetime!(2026-05-27 09:10 UTC));
    }

    #[test]
    fn segments_splits_on_active_state_change() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 09:00 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 09:02 UTC), "Safari", false),
            app_obs(datetime!(2026-05-27 09:04 UTC), "Safari", true),
        ];
        let segments = aggregate_into_segments(
            &observations,
            day_start(),
            datetime!(2026-05-27 09:06 UTC),
            Duration::minutes(5),
        );
        assert_eq!(segments.len(), 3);
        assert!(segments[0].is_active);
        assert!(!segments[1].is_active);
        assert!(segments[2].is_active);
    }

    #[test]
    fn segments_caps_long_gap_leaving_visual_gap() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 09:00 UTC), "Safari", true),
            // 2-hour gap (laptop slept) — span should be clipped to max_gap
            app_obs(datetime!(2026-05-27 11:00 UTC), "Safari", true),
        ];
        let segments = aggregate_into_segments(
            &observations,
            day_start(),
            datetime!(2026-05-27 11:05 UTC),
            Duration::minutes(2),
        );
        // Two segments because the gap means they don't meet end-to-end.
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].starts_at, datetime!(2026-05-27 09:00 UTC));
        assert_eq!(segments[0].ends_at, datetime!(2026-05-27 09:02 UTC));
        assert_eq!(segments[1].starts_at, datetime!(2026-05-27 11:00 UTC));
        assert_eq!(segments[1].ends_at, datetime!(2026-05-27 11:02 UTC));
    }

    #[test]
    fn segments_clips_to_day_window() {
        let observations = vec![
            app_obs(datetime!(2026-05-26 23:55 UTC), "Safari", true),
            app_obs(datetime!(2026-05-27 00:30 UTC), "Code", true),
        ];
        let segments = aggregate_into_segments(
            &observations,
            day_start(),
            datetime!(2026-05-27 00:35 UTC),
            Duration::hours(2),
        );
        assert_eq!(segments.len(), 2);
        // Safari starts before day_start → clipped
        assert_eq!(segments[0].starts_at, day_start());
        assert_eq!(segments[0].ends_at, datetime!(2026-05-27 00:30 UTC));
        assert_eq!(segments[1].app_name, "Code");
    }

    #[test]
    fn segments_empty_input_yields_empty_output() {
        let segments = aggregate_into_segments(
            &[],
            day_start(),
            datetime!(2026-05-27 12:00 UTC),
            Duration::minutes(2),
        );
        assert!(segments.is_empty());
    }

    #[test]
    fn result_is_sorted_by_active_seconds_descending() {
        let observations = vec![
            app_obs(datetime!(2026-05-27 12:00 UTC), "Brief", true),
            app_obs(datetime!(2026-05-27 12:01 UTC), "Long", true),
        ];
        let until = datetime!(2026-05-27 12:11 UTC);
        let result = aggregate_by_app(&observations, until, Duration::minutes(15));

        assert_eq!(result[0].app_name, "Long");
        assert_eq!(result[1].app_name, "Brief");
    }
}
