//! Model choice per stage: `models.toml` compiled in, then `AI_*` env overrides.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub const DEFAULT_MODELS_TOML: &str = include_str!("../models.toml");

/// Sent as OpenRouter's `reasoning.effort`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    None,
    Low,
    Medium,
    High,
}

impl Effort {
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::None => "none",
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }
}

impl fmt::Display for Effort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Effort {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Effort::None),
            "low" => Ok(Effort::Low),
            "medium" => Ok(Effort::Medium),
            "high" => Ok(Effort::High),
            other => Err(format!(
                "unknown effort `{other}`; expected none, low, medium or high"
            )),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Intake,
    Responder,
    Review,
    Notice,
}

impl Stage {
    pub const ALL: [Stage; 4] = [
        Stage::Intake,
        Stage::Responder,
        Stage::Review,
        Stage::Notice,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Intake => "intake",
            Stage::Responder => "responder",
            Stage::Review => "review",
            Stage::Notice => "notice",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageModel {
    pub model: String,
    pub effort: Effort,
    pub timeout_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    pub intake: StageModel,
    pub responder: StageModel,
    pub review: StageModel,
    pub notice: StageModel,
    pub fallback_model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("models.toml is invalid: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("{var}: {message}")]
    Env { var: String, message: String },
}

impl AiConfig {
    /// The compiled-in defaults with the process environment applied.
    pub fn load() -> Result<AiConfig, ConfigError> {
        Self::from_sources(DEFAULT_MODELS_TOML, |var| std::env::var(var).ok())
    }

    /// `env` returns a variable's value; blank values are treated as unset so
    /// that a commented-out `.env` line and an empty one behave the same.
    pub fn from_sources(
        toml_text: &str,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<AiConfig, ConfigError> {
        let mut config: AiConfig = toml::from_str(toml_text)?;
        let get = |var: &str| {
            env(var)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        for stage in Stage::ALL {
            let prefix = format!("AI_{}", stage.as_str().to_uppercase());
            let target = config.stage_mut(stage);
            if let Some(model) = get(&format!("{prefix}_MODEL")) {
                target.model = model;
            }
            let var = format!("{prefix}_EFFORT");
            if let Some(effort) = get(&var) {
                target.effort = effort
                    .parse()
                    .map_err(|message| ConfigError::Env { var, message })?;
            }
        }
        if let Some(model) = get("AI_FALLBACK_MODEL") {
            config.fallback_model = model;
        }
        Ok(config)
    }

    pub fn stage(&self, stage: Stage) -> &StageModel {
        match stage {
            Stage::Intake => &self.intake,
            Stage::Responder => &self.responder,
            Stage::Review => &self.review,
            Stage::Notice => &self.notice,
        }
    }

    fn stage_mut(&mut self, stage: Stage) -> &mut StageModel {
        match stage {
            Stage::Intake => &mut self.intake,
            Stage::Responder => &mut self.responder,
            Stage::Review => &mut self.review,
            Stage::Notice => &mut self.notice,
        }
    }

    /// The retry model for a stage: same effort and timeout, different model.
    pub fn fallback_for(&self, stage: Stage) -> StageModel {
        StageModel {
            model: self.fallback_model.clone(),
            ..self.stage(stage).clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn load(vars: &[(&str, &str)]) -> Result<AiConfig, ConfigError> {
        let env: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        AiConfig::from_sources(DEFAULT_MODELS_TOML, |k| env.get(k).cloned())
    }

    #[test]
    fn defaults_come_from_models_toml() {
        let c = load(&[]).unwrap();
        assert_eq!(
            c.intake,
            StageModel {
                model: "openai/gpt-6-luna".into(),
                effort: Effort::Low,
                timeout_secs: 30
            }
        );
        assert_eq!(c.responder.effort, Effort::None);
        assert_eq!(
            (
                c.review.model.as_str(),
                c.review.effort,
                c.review.timeout_secs
            ),
            ("openai/gpt-6-luna-pro", Effort::Medium, 90)
        );
        assert_eq!(c.fallback_model, "openai/gpt-5.6-luna");
    }

    #[test]
    fn env_overrides_win_and_blank_values_are_ignored() {
        let c = load(&[
            ("AI_INTAKE_MODEL", "openai/other"),
            ("AI_INTAKE_EFFORT", " high "),
            ("AI_RESPONDER_MODEL", ""),
            ("AI_REVIEW_EFFORT", "   "),
            ("AI_FALLBACK_MODEL", "openai/backup"),
        ])
        .unwrap();
        assert_eq!(c.intake.model, "openai/other");
        assert_eq!(c.intake.effort, Effort::High);
        assert_eq!(c.responder.model, "openai/gpt-6-luna");
        assert_eq!(c.review.effort, Effort::Medium);
        assert_eq!(c.fallback_model, "openai/backup");
    }

    #[test]
    fn invalid_effort_names_the_variable() {
        let err = load(&[("AI_REVIEW_EFFORT", "max")]).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("AI_REVIEW_EFFORT: unknown effort `max`")
        );
    }

    #[test]
    fn fallback_keeps_the_stage_effort_and_timeout() {
        let c = load(&[]).unwrap();
        let f = c.fallback_for(Stage::Review);
        assert_eq!(
            (f.model.as_str(), f.effort, f.timeout_secs),
            ("openai/gpt-5.6-luna", Effort::Medium, 90)
        );
    }
}
