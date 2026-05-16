use anyhow::Result;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRule {
    pub pattern: String,
    pub title: String,
    pub suggest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnosis {
    pub task: String,
    pub title: String,
    pub suggest: String,
    pub evidence: Vec<String>,
}

pub struct DiagnosticEngine {
    rules: Vec<CompiledRule>,
}

struct CompiledRule {
    regex: Regex,
    title: String,
    suggest: String,
}

impl DiagnosticEngine {
    pub fn with_builtin_rules(mut user_rules: Vec<DiagnosticRule>) -> Result<Self> {
        user_rules.extend(builtin_rules());
        let mut rules = Vec::new();
        for rule in user_rules {
            rules.push(CompiledRule {
                regex: Regex::new(&format!("(?i){}", rule.pattern))?,
                title: rule.title,
                suggest: rule.suggest,
            });
        }
        Ok(Self { rules })
    }

    pub fn diagnose(&self, task: &str, logs: &[String]) -> Option<Diagnosis> {
        for rule in &self.rules {
            let evidence: Vec<String> = logs
                .iter()
                .rev()
                .filter(|line| rule.regex.is_match(line))
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();

            if !evidence.is_empty() {
                return Some(Diagnosis {
                    task: task.to_string(),
                    title: rule.title.clone(),
                    suggest: rule.suggest.clone(),
                    evidence,
                });
            }
        }
        None
    }
}

fn builtin_rules() -> Vec<DiagnosticRule> {
    vec![
        DiagnosticRule {
            pattern: r"EADDRINUSE|address already in use".into(),
            title: "Port already in use".into(),
            suggest: "Run `lsof -i :<port>` or change the port.".into(),
        },
        DiagnosticRule {
            pattern: "connection refused".into(),
            title: "Dependency not ready".into(),
            suggest: "Check readiness probes and dependency order.".into(),
        },
        DiagnosticRule {
            pattern: r"Cannot find module|Module not found".into(),
            title: "Missing dependency".into(),
            suggest: "Run the package install command for this project.".into(),
        },
        DiagnosticRule {
            pattern: r"database system is starting up".into(),
            title: "Database still starting".into(),
            suggest: "Wait for database readiness or increase startup timeout.".into(),
        },
    ]
}
