use chrono::{Local, NaiveTime};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Scheduled tasks: cron-like task scheduler.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub name: String,
    pub action: ScheduledAction,
    pub schedule: Schedule,
    pub enabled: bool,
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
    last_check: std::time::Instant,
}

impl Scheduler {
    pub fn new() -> Self { Self { tasks: Vec::new(), last_check: std::time::Instant::now() } }

    pub fn add(&mut self, task: ScheduledTask) {
        info!(name = %task.name, "Scheduled task added");
        self.tasks.push(task);
    }

    /// Check and return tasks that should fire now.
    pub fn check(&mut self) -> Vec<ScheduledAction> {
        let now = Local::now().time();
        let elapsed = self.last_check.elapsed();
        self.last_check = std::time::Instant::now();

        let mut due = Vec::new();
        for task in &self.tasks {
            if !task.enabled { continue; }
            match &task.schedule {
                Schedule::Once(time) => {
                    let diff = (now - *time).num_seconds().abs();
                    if diff < 2 { due.push(task.action.clone()); }
                }
                Schedule::Interval { every_secs } => {
                    if elapsed.as_secs() >= *every_secs { due.push(task.action.clone()); }
                }
                Schedule::Daily(time) => {
                    let diff = (now - *time).num_seconds().abs();
                    if diff < 2 { due.push(task.action.clone()); }
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
}
