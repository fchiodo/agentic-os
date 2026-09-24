use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Work,
    Planphysique,
    Personal,
    Family,
    Finance,
    Research,
}

impl Domain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Domain::Work => "work",
            Domain::Planphysique => "planphysique",
            Domain::Personal => "personal",
            Domain::Family => "family",
            Domain::Finance => "finance",
            Domain::Research => "research",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlStatus {
    pub pending_memory_proposals: i64,
}
