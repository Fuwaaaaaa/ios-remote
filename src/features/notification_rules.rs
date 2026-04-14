use serde::{Deserialize, Serialize};

/// Notification filter rules: control which notifications to capture/forward.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotificationRule {
    pub name: String,
    pub condition: RuleCondition,
    pub action: RuleAction,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RuleCondition {
    /// Match any notification
    Any,
    /// Match by app name (OCR-detected from notification banner)
    AppName(String),
    /// Match by text content (contains keyword)
    Contains(String),
    /// Exclude by keyword
    NotContains(String),
    /// Match multiple conditions (AND)
    All(Vec<RuleCondition>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RuleAction {
    Capture,
    Forward,
    Ignore,
    PlaySound(String),
    ForwardToWebhook(String),
}

pub struct NotificationRuleEngine {
    rules: Vec<NotificationRule>,
}

impl NotificationRuleEngine {
    pub fn new() -> Self { Self { rules: Vec::new() } }

    pub fn add_rule(&mut self, rule: NotificationRule) { self.rules.push(rule); }

    /// Evaluate rules against a notification text. Returns matching actions.
    pub fn evaluate(&self, text: &str) -> Vec<RuleAction> {
        self.rules.iter()
            .filter(|r| r.enabled && Self::matches(&r.condition, text))
            .map(|r| r.action.clone())
            .collect()
    }

    fn matches(condition: &RuleCondition, text: &str) -> bool {
        match condition {
            RuleCondition::Any => true,
            RuleCondition::AppName(app) => text.to_lowercase().contains(&app.to_lowercase()),
            RuleCondition::Contains(kw) => text.to_lowercase().contains(&kw.to_lowercase()),
            RuleCondition::NotContains(kw) => !text.to_lowercase().contains(&kw.to_lowercase()),
            RuleCondition::All(conditions) => conditions.iter().all(|c| Self::matches(c, text)),
        }
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.rules).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn load(&mut self, path: &str) -> Result<(), String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        self.rules = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        Ok(())
    }
}
