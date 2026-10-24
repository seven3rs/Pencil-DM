use chrono::{DateTime, Datelike, LocalResult, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleRule {
    pub id: String,
    pub name: String,
    /// ISO weekday numbers: Monday=1 through Sunday=7.
    pub days: Vec<u32>,
    pub start_time: String,
    pub stop_time: String,
    pub timezone: String,
    pub speed_limit: Option<u64>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleEventKind {
    Start,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleEvent {
    pub at: DateTime<Utc>,
    pub rule_id: String,
    pub kind: ScheduleEventKind,
    pub speed_limit: Option<u64>,
}

fn weekday_number(day: Weekday) -> u32 {
    day.number_from_monday()
}

fn resolve_local<T: TimeZone>(tz: &T, local: chrono::NaiveDateTime) -> Option<DateTime<T>> {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(value) => Some(value),
        // During a fall-back overlap, choose the earlier occurrence so the rule
        // has one deterministic transition and never starts twice.
        LocalResult::Ambiguous(a, b) => Some(if a.timestamp() <= b.timestamp() { a } else { b }),
        // A wall-clock time inside a spring-forward gap never occurs.
        LocalResult::None => None,
    }
}

/// Returns the next start/stop transition after `now`, considering each rule's
/// IANA time zone. Local times skipped by DST are skipped; ambiguous times use
/// the first occurrence. The function is pure and does not read system state.
pub fn next_event(now: DateTime<Utc>, rules: &[ScheduleRule]) -> Option<ScheduleEvent> {
    let mut best: Option<ScheduleEvent> = None;
    for rule in rules.iter().filter(|rule| rule.enabled) {
        let tz: Tz = match rule.timezone.parse() {
            Ok(tz) => tz,
            Err(_) => continue,
        };
        let start = match NaiveTime::parse_from_str(&rule.start_time, "%H:%M") {
            Ok(time) => time,
            Err(_) => continue,
        };
        let stop = match NaiveTime::parse_from_str(&rule.stop_time, "%H:%M") {
            Ok(time) => time,
            Err(_) => continue,
        };
        for offset in -1..=8 {
            let date = now.with_timezone(&tz).date_naive() + chrono::Duration::days(offset);
            if !rule.days.contains(&weekday_number(date.weekday())) {
                continue;
            }
            for (time, kind) in [
                (start, ScheduleEventKind::Start),
                (stop, ScheduleEventKind::Stop),
            ] {
                let event_date = if kind == ScheduleEventKind::Stop && stop <= start {
                    date + chrono::Duration::days(1)
                } else {
                    date
                };
                let local = event_date.and_time(time);
                let Some(local_dt) = resolve_local(&tz, local) else {
                    continue;
                };
                let at = local_dt.with_timezone(&Utc);
                if at <= now {
                    continue;
                }
                let candidate = ScheduleEvent {
                    at,
                    rule_id: rule.id.clone(),
                    kind,
                    speed_limit: rule.speed_limit,
                };
                if best
                    .as_ref()
                    .is_none_or(|current| candidate.at < current.at)
                {
                    best = Some(candidate);
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn rule(zone: &str, day: u32, start: &str, stop: &str) -> ScheduleRule {
        ScheduleRule {
            id: "r1".into(),
            name: "test".into(),
            days: vec![day],
            start_time: start.into(),
            stop_time: stop.into(),
            timezone: zone.into(),
            speed_limit: Some(1000),
            enabled: true,
        }
    }

    #[test]
    fn finds_next_event_in_rule_timezone() {
        let now = Utc.with_ymd_and_hms(2026, 1, 5, 8, 0, 0).unwrap();
        let event = next_event(now, &[rule("America/New_York", 1, "09:00", "10:00")]).unwrap();
        assert_eq!(
            event.at,
            Utc.with_ymd_and_hms(2026, 1, 5, 14, 0, 0).unwrap()
        );
        assert_eq!(event.kind, ScheduleEventKind::Start);
    }

    #[test]
    fn handles_spring_forward_gap() {
        // 02:30 does not exist in New York on the 2026 spring-forward date.
        let now = Utc.with_ymd_and_hms(2026, 3, 8, 5, 0, 0).unwrap();
        let event = next_event(now, &[rule("America/New_York", 7, "02:30", "04:00")]).unwrap();
        assert_eq!(event.kind, ScheduleEventKind::Stop);
        assert_eq!(event.at, Utc.with_ymd_and_hms(2026, 3, 8, 8, 0, 0).unwrap());
    }

    #[test]
    fn finds_stop_for_an_overnight_window_started_yesterday() {
        let now = Utc.with_ymd_and_hms(2026, 1, 6, 1, 0, 0).unwrap();
        let event = next_event(now, &[rule("UTC", 1, "22:00", "02:00")]).unwrap();
        assert_eq!(event.kind, ScheduleEventKind::Stop);
        assert_eq!(event.at, Utc.with_ymd_and_hms(2026, 1, 6, 2, 0, 0).unwrap());
    }

    #[test]
    fn chooses_first_fall_back_occurrence() {
        let now = Utc.with_ymd_and_hms(2026, 11, 1, 4, 0, 0).unwrap();
        let event = next_event(now, &[rule("America/New_York", 7, "01:30", "03:00")]).unwrap();
        assert_eq!(event.kind, ScheduleEventKind::Start);
        assert_eq!(
            event.at,
            Utc.with_ymd_and_hms(2026, 11, 1, 5, 30, 0).unwrap()
        );
    }
}
