use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub generated_at: i64,
    pub catalog: CatalogSection,
    pub sources: Vec<SourceDescriptor>,
    pub runtime: RuntimeInfo,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSection {
    pub counts: CatalogCounts,
    pub items: Vec<CatalogItem>,
    pub total_items: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogCounts {
    pub agent: i64,
    pub automation: i64,
    pub mcp: i64,
    pub plugin: i64,
    pub prompt: i64,
    pub routine: i64,
    pub skill: i64,
    pub workflow: i64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CatalogKind {
    Agent,
    Automation,
    Mcp,
    Plugin,
    Prompt,
    Routine,
    Skill,
    Workflow,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub id: String,
    pub kind: CatalogKind,
    pub name: String,
    pub display_name: String,
    pub summary: Option<String>,
    pub path: String,
    pub origin: String,
    pub group: String,
    pub tags: Vec<String>,
    pub version: Option<String>,
    pub category: Option<String>,
    pub updated_at: Option<i64>,
    pub provider: String,
    pub detector: String,
    pub entrypoint: Option<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDescriptor {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub path: String,
    pub status: SourceStatus,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceStatus {
    Available,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub platform: String,
    pub codex_home: String,
}
