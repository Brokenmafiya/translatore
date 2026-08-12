use serde::Deserialize;
use worker::*;

/// Rule types for the routing allow-list
#[derive(Deserialize, Clone, Debug)]
pub struct Rule {
    pub pattern: String,
    #[serde(default = "default_type")]
    pub rule_type: String,
}

fn default_type() -> String {
    "http".to_string()
}

/// Simple router that loads rules from KV and does glob matching
pub struct Router {
    rules: Vec<Rule>,
}

impl Router {
    /// Load rules from KV. The ROUTES namespace stores a single key "RULESET"
    /// containing a JSON array of rules.
    pub async fn load(env: &Env) -> Result<Self> {
        let kv = env.kv("ROUTES")?;

        let rules: Vec<Rule> = match kv.get("RULESET").json().await? {
            Some(r) => r,
            None => {
                // No rules = allow nothing (safe default)
                vec![]
            }
        };

        Ok(Router { rules })
    }

    /// Check if a target is allowed for the given request type
    pub fn is_allowed(&self, target: &str, rule_type: &str) -> bool {
        self.rules.iter().any(|rule| {
            (rule.rule_type == rule_type || rule.rule_type == "*")
                && glob_match(&rule.pattern, target)
        })
    }
}

/// Simple glob matching: supports * (any chars) and ? (single char)
/// Examples: "*.github.com" matches "api.github.com"
///           "192.168.1.*:*" matches "192.168.1.50:22"
fn glob_match(pattern: &str, text: &str) -> bool {
    glob_match_recursive(pattern.as_bytes(), text.as_bytes(), 0, 0)
}

fn glob_match_recursive(pattern: &[u8], text: &[u8], mut pi: usize, mut ti: usize) -> bool {
    while pi < pattern.len() {
        if pattern[pi] == b'*' {
            // Skip consecutive stars
            while pi < pattern.len() && pattern[pi] == b'*' {
                pi += 1;
            }
            // Star at end matches everything
            if pi >= pattern.len() {
                return true;
            }
            // Try matching rest of pattern at every position
            for i in ti..=text.len() {
                if glob_match_recursive(pattern, text, pi, i) {
                    return true;
                }
            }
            return false;
        } else if ti >= text.len() {
            return false;
        } else if pattern[pi] == b'?' || pattern[pi] == text[ti] {
            pi += 1;
            ti += 1;
        } else {
            return false;
        }
    }
    ti >= text.len()
}
