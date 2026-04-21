use chrono::{Local, NaiveDate, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::info;

/// Scheduled tasks: cron-like task scheduler with 1-second resolution.
///
/// Semantics:
/// - `Once(time)`: fires once per process lifetime, on the first tick within the
///   1-second window around the target time.
/// - `Daily(time)`: fires once per calendar day, tracked by `last_fired_date`.
/// - `Interval { every_secs }`: fires every N seconds, tracked per-task via
///   `last_fired_instant` so registration order and drop-outs don't cause drift.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub name: String,
    pub action: ScheduledAction,
    pub schedule: Schedule,
    pub enabled: bool,

    /// Date this task last fired (Daily only). Internal state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_fired_date: Option<NaiveDate>,

    /// Whether a `Once` task has already fired. Internal state.
    #[serde(default)]
    once_fired: bool,

    /// Monotonic instant of the last Interval fire. Not persisted.
    #[serde(skip)]
    last_fired_instant: Option<Instant>,
}

impl ScheduledTask {
    pub fn new(name: impl Into<String>, action: ScheduledAction, schedule: Schedule) -> Self {
        Self {
            name: name.into(),
            action,
            schedule,
            enabled: true,
            last_fired_date: None,
            once_fired: false,
            last_fired_instant: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ScheduledAction {
    Screenshot,
    StartRecording,
    StopRecording,
    RunMacro(String),
    RunScript(String),
    Webhook(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Schedule {
    Once(NaiveTime),
    Interval { every_secs: u64 },
    Daily(NaiveTime),
}

pub struct Scheduler {
    tasks: Vec<ScheduledTask>,
}

impl Scheduler {
    pub fn new() -> Self { Self { tasks: Vec::new() } }

    pub fn add(&mut self, task: ScheduledTask) {
        info!(name = %task.name, "Scheduled task added");
        self.tasks.push(task);
    }

    /// Check and return tasks that should fire now. Call this once per second.
    pub fn check(&mut self) -> Vec<ScheduledAction> {
        let now_dt = Local::now();
        let now_time = now_dt.time();
        let today = now_dt.date_naive();
        let now_instant = Instant::now();

        let mut due = Vec::new();
        for task in &mut self.tasks {
            if !task.enabled {
                continue;
            }
            match &task.schedule {
                Schedule::Once(time) => {
                    if task.once_fired {
                        continue;
                    }
                    if within_one_second(now_time, *time) {
                        due.push(task.action.clone());
                        task.once_fired = true;
                    }
                }
                Schedule::Daily(time) => {
                    if task.last_fired_date == Some(today) {
                        continue;
                    }
                    if within_one_second(now_time, *time) {
                        due.push(task.action.clone());
                        task.last_fired_date = Some(today);
                    }
                }
                Schedule::Interval { every_secs } => {
                    let should_fire = match task.last_fired_instant {
                        None => true,
                        Some(last) => {
                            now_instant.saturating_duration_since(last).as_secs()
                                >= *every_secs
                        }
                    };
                    if should_fire {
                        due.push(task.action.clone());
                        task.last_fired_instant = Some(now_instant);
                    }
                }
            }
        }
        due
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.tasks).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn load(&mut self, path: &str) -> Result<(), String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        self.tasks = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[cfg(test)]
    pub fn tasks_mut(&mut self) -> &mut Vec<ScheduledTask> {
        &mut self.tasks
    }
}

/// True when `now` is within ±1 second of the target time-of-day, handling
/// midnight wrap-around by comparing through a 24-hour modular distance.
fn within_one_second(now: NaiveTime, target: NaiveTime) -> bool {
    let now_s = now.num_seconds_from_midnight() as i64;
    let tgt_s = target.num_seconds_from_midnight() as i64;
    let diff = (now_s - tgt_s).abs().min(86_400 - (now_s - tgt_s).abs());
    diff <= 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    #[test]
    fn once_fires_at_most_once() {
        let mut s = Scheduler::new();
        let t = Local::now().time();
        s.add(ScheduledTask::new(
            "once",
            ScheduledAction::Screenshot,
            Schedule::Once(t),
        ));
        assert_eq!(s.check().len(), 1);
        assert_eq!(s.check().len(), 0);
    }

    #[test]
    fn daily_fires_only_once_per_day() {
        let mut s = Scheduler::new();
        let t = Local::now().time();
        s.add(ScheduledTask::new(
            "daily",
            ScheduledAction::Screenshot,
            Schedule::Daily(t),
        ));
        let first = s.check();
        let second = s.check();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 0);
        // Force "yesterday" → next check should fire again.
        s.tasks_mut()[0].last_fired_date =
            Some(Local::now().date_naive().pred_opt().unwrap());
        assert_eq!(s.check().len(), 1);
    }

    #[test]
    fn interval_respects_every_secs() {
        let mut s = Scheduler::new();
        s.add(ScheduledTask::new(
            "every",
            ScheduledAction::Screenshot,
            Schedule::Interval { every_secs: 60 },
        ));
        assert_eq!(s.check().len(), 1); // first call always fires
        assert_eq!(s.check().len(), 0); // too soon
        // Pretend the last fire was 2 minutes ago.
        s.tasks_mut()[0].last_fired_instant =
            Some(Instant::now() - std::time::Duration::from_secs(120));
        assert_eq!(s.check().len(), 1);
    }

    #[test]
    fn one_second_window_wraps_midnight() {
        let a = NaiveTime::from_hms_opt(23, 59, 59).unwrap();
        let b = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        assert!(within_one_second(a, b));
        assert!(within_one_second(b, a));
    }
}
