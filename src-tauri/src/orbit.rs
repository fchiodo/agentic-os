use std::collections::{BTreeMap, HashMap};

use rusqlite::params;
use serde::Serialize;
use serde_json::Value;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::{CatalogItem, CatalogKind};

const DOMAINS: [&str; 6] = [
    "work",
    "planphysique",
    "personal",
    "family",
    "finance",
    "research",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbitMap {
    pub generated_at: String,
    pub nodes: Vec<OrbitNode>,
    pub edges: Vec<OrbitEdge>,
    pub counts: OrbitCounts,
    pub metrics: OrbitMetrics,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbitMetrics {
    pub compose_ms: f64,
    pub tasks_scanned: usize,
    pub traces_scanned: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbitCounts {
    pub skills: usize,
    pub memories: usize,
    pub routines: usize,
    pub applications: usize,
    pub relations: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbitNode {
    pub id: String,
    pub kind: String,
    pub ring: u8,
    pub label: String,
    pub subtitle: Option<String>,
    pub domain: Option<String>,
    pub sensitivity: Option<String>,
    pub status: String,
    pub source_path: Option<String>,
    pub source_ref: String,
    pub group_id: Option<String>,
    pub count: usize,
    pub preview: Option<String>,
    pub updated_at: Option<String>,
    pub actions: Vec<String>,
    pub aggregate: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbitEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub relation: String,
    pub evidence: String,
    pub weight: usize,
    pub activity_at: Option<String>,
    pub provenance: Vec<OrbitProvenance>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbitProvenance {
    pub kind: String,
    pub reference: String,
    pub detail: String,
    pub ts: Option<String>,
}

#[derive(Debug, Clone)]
struct TaskRecord {
    id: String,
    title: String,
    goal: String,
}

#[derive(Debug, Clone)]
struct TraceRecord {
    run_id: String,
    task_id: Option<String>,
    ts: String,
    kind: String,
    summary: String,
    detail: Value,
}

/// Compose the operational map from the authoritative local registries.
/// Filtering happens here, before node counts, previews, and trace relations
/// are produced, so hidden domains or sensitive memories cannot leak through
/// aggregate metadata.
pub fn build(db: &Db, domain: Option<&str>, include_sensitive: bool) -> AppResult<OrbitMap> {
    let compose_started = std::time::Instant::now();
    if domain.is_some_and(|value| !DOMAINS.contains(&value)) {
        return Err(AppError::Io(std::io::Error::other(
            "invalid orbit domain filter",
        )));
    }

    crate::memory::index::ensure_tables(db)?;
    let catalog = crate::discovery::discover()?.catalog.items;
    let memories = crate::memory::index::list_all(db)?
        .into_iter()
        .filter(|memory| domain.map_or(true, |value| memory.domain == value))
        .filter(|memory| include_sensitive || memory.sensitivity != "sensitive")
        .filter(|memory| memory.status != "expired")
        .collect::<Vec<_>>();

    let mut nodes = vec![OrbitNode {
        id: "core:agentic-os".to_string(),
        kind: "core".to_string(),
        ring: 0,
        label: "AgenticOS".to_string(),
        subtitle: Some("Local control plane".to_string()),
        domain: None,
        sensitivity: None,
        status: "active".to_string(),
        source_path: None,
        source_ref: "runtime:agentic-os".to_string(),
        group_id: None,
        count: 1,
        preview: Some("Deterministic shell, governed memory, local execution.".to_string()),
        updated_at: None,
        actions: Vec::new(),
        aggregate: false,
    }];
    let mut edges = Vec::new();

    let skill_items = catalog
        .iter()
        .filter(|item| item.kind == CatalogKind::Skill)
        .cloned()
        .collect::<Vec<_>>();
    let routine_items = catalog
        .iter()
        .filter(|item| matches!(item.kind, CatalogKind::Routine | CatalogKind::Workflow))
        .cloned()
        .collect::<Vec<_>>();
    let application_items = catalog
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                CatalogKind::Plugin | CatalogKind::Mcp | CatalogKind::Automation
            )
        })
        .cloned()
        .collect::<Vec<_>>();

    add_catalog_ring(&mut nodes, &mut edges, &skill_items, 1, "skill");
    add_memory_ring(&mut nodes, &mut edges, &memories, domain);
    add_catalog_ring(&mut nodes, &mut edges, &routine_items, 3, "routine");
    add_catalog_ring(&mut nodes, &mut edges, &application_items, 4, "application");
    add_temporal_relations(&nodes, &mut edges);

    let (tasks_scanned, traces_scanned) = add_observed_relations(
        db,
        &nodes,
        &routine_items,
        &skill_items,
        &application_items,
        &mut edges,
    )?;

    let node_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    edges.retain(|edge| {
        node_ids.contains(edge.source.as_str()) && node_ids.contains(edge.target.as_str())
    });

    let counts = OrbitCounts {
        skills: skill_items.len(),
        memories: memories.len(),
        routines: routine_items.len(),
        applications: application_items.len(),
        relations: edges.len(),
    };

    Ok(OrbitMap {
        generated_at: chrono::Utc::now().to_rfc3339(),
        nodes,
        edges,
        counts,
        metrics: OrbitMetrics {
            compose_ms: compose_started.elapsed().as_secs_f64() * 1000.0,
            tasks_scanned,
            traces_scanned,
        },
    })
}

fn add_temporal_relations(nodes: &[OrbitNode], edges: &mut Vec<OrbitEdge>) {
    let memory_by_path = nodes
        .iter()
        .filter(|node| node.kind == "memory")
        .filter_map(|node| {
            node.source_path
                .as_ref()
                .map(|path| (path.as_str(), node.id.as_str()))
        })
        .collect::<HashMap<_, _>>();
    let memory_ids = nodes
        .iter()
        .filter(|node| node.kind == "memory")
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();

    for node in nodes.iter().filter(|node| node.kind == "memory") {
        let Some(path) = node.source_path.as_deref() else {
            continue;
        };
        let Ok((content, _)) = crate::memory::vault::read_file(path) else {
            continue;
        };
        let Some((frontmatter, _)) = crate::memory::frontmatter::parse(&content) else {
            continue;
        };
        if let Some(previous_id) = frontmatter.supersedes.as_deref() {
            let target = format!("memory:{previous_id}");
            if memory_ids.contains(target.as_str()) {
                edges.push(declared_edge(
                    &node.id,
                    &target,
                    "supersedes",
                    path,
                    "Temporal replacement declared in Markdown frontmatter",
                ));
            }
        }
        for source in frontmatter.sources {
            if let Some(target) = memory_by_path.get(source.as_str()) {
                edges.push(declared_edge(
                    &node.id,
                    target,
                    "derived_from",
                    path,
                    &format!("Original Ask source: {source}"),
                ));
            }
        }
    }
}

fn add_catalog_ring(
    nodes: &mut Vec<OrbitNode>,
    edges: &mut Vec<OrbitEdge>,
    items: &[CatalogItem],
    ring: u8,
    kind: &str,
) {
    let mut groups: BTreeMap<String, Vec<&CatalogItem>> = BTreeMap::new();
    for item in items {
        let label = if item.group.trim().is_empty() {
            item.provider.trim()
        } else {
            item.group.trim()
        };
        groups.entry(label.to_string()).or_default().push(item);
    }

    for (group_label, group_items) in groups {
        let group_id = format!("group:{kind}:{}", stable_key(&group_label));
        nodes.push(OrbitNode {
            id: group_id.clone(),
            kind: format!("{kind}_group"),
            ring,
            label: group_label.clone(),
            subtitle: Some(format!("{} {}", group_items.len(), plural_label(kind))),
            domain: None,
            sensitivity: None,
            status: "active".to_string(),
            source_path: None,
            source_ref: format!("registry-group:{kind}:{group_label}"),
            group_id: None,
            count: group_items.len(),
            preview: Some(format!(
                "Aggregate derived from {} registered {}.",
                group_items.len(),
                plural_label(kind)
            )),
            updated_at: None,
            actions: vec!["expand".to_string()],
            aggregate: true,
        });
        edges.push(declared_edge(
            "core:agentic-os",
            &group_id,
            "registers",
            &format!("catalog:{kind}:{group_label}"),
            &format!("{} registered items", group_items.len()),
        ));

        for item in group_items {
            let node_id = format!("{kind}:{}", item.id);
            nodes.push(OrbitNode {
                id: node_id.clone(),
                kind: kind.to_string(),
                ring,
                label: item.display_name.clone(),
                subtitle: Some(item.origin.clone()),
                domain: None,
                sensitivity: None,
                status: "active".to_string(),
                source_path: Some(item.path.clone()),
                source_ref: format!("catalog:{}", item.id),
                group_id: Some(group_id.clone()),
                count: 1,
                preview: item.summary.clone(),
                updated_at: item.updated_at.map(|value| value.to_string()),
                actions: vec!["inspect_source".to_string()],
                aggregate: false,
            });
            edges.push(declared_edge(
                &group_id,
                &node_id,
                "contains",
                &format!("catalog:{}", item.id),
                &format!("Discovered by {}", item.detector),
            ));
        }
    }
}

fn add_memory_ring(
    nodes: &mut Vec<OrbitNode>,
    edges: &mut Vec<OrbitEdge>,
    memories: &[crate::memory::MemoryRow],
    domain_filter: Option<&str>,
) {
    for domain in DOMAINS {
        if domain_filter.is_some_and(|value| value != domain) {
            continue;
        }
        let domain_memories = memories
            .iter()
            .filter(|memory| memory.domain == domain)
            .collect::<Vec<_>>();
        let domain_id = format!("memory-domain:{domain}");
        nodes.push(OrbitNode {
            id: domain_id.clone(),
            kind: "memory_domain".to_string(),
            ring: 2,
            label: domain_label(domain).to_string(),
            subtitle: Some(format!("{} memories", domain_memories.len())),
            domain: Some(domain.to_string()),
            sensitivity: None,
            status: "active".to_string(),
            source_path: None,
            source_ref: format!("vault-domain:{domain}"),
            group_id: None,
            count: domain_memories.len(),
            preview: Some(format!(
                "Domain aggregate after lifecycle and sensitivity filters: {} visible.",
                domain_memories.len()
            )),
            updated_at: None,
            actions: vec!["expand".to_string()],
            aggregate: true,
        });
        edges.push(declared_edge(
            "core:agentic-os",
            &domain_id,
            "governs",
            &format!("vault-domain:{domain}"),
            "Domain fence declared by the memory registry",
        ));

        for memory in domain_memories {
            let node_id = format!("memory:{}", memory.id);
            let mut actions = vec!["open_memory".to_string()];
            if memory.status == "stale" {
                actions.push("confirm_memory".to_string());
            }
            nodes.push(OrbitNode {
                id: node_id.clone(),
                kind: "memory".to_string(),
                ring: 2,
                label: memory.title.clone(),
                subtitle: Some(memory.mem_type.clone()),
                domain: Some(memory.domain.clone()),
                sensitivity: Some(memory.sensitivity.clone()),
                status: memory.status.clone(),
                source_path: Some(memory.vault_path.clone()),
                source_ref: format!("memory:{}", memory.id),
                group_id: Some(domain_id.clone()),
                count: 1,
                preview: memory.summary.clone(),
                updated_at: Some(memory.updated_at.clone()),
                actions,
                aggregate: false,
            });
            edges.push(declared_edge(
                &domain_id,
                &node_id,
                "contains",
                &memory.vault_path,
                "Domain and provenance declared in Markdown frontmatter",
            ));
        }
    }
}

fn add_observed_relations(
    db: &Db,
    nodes: &[OrbitNode],
    routines: &[CatalogItem],
    skills: &[CatalogItem],
    applications: &[CatalogItem],
    edges: &mut Vec<OrbitEdge>,
) -> AppResult<(usize, usize)> {
    let tasks = load_tasks(db)?;
    let traces = load_traces(db)?;
    let tasks_scanned = tasks.len();
    let traces_scanned = traces.len();
    let memory_by_path = nodes
        .iter()
        .filter(|node| node.kind == "memory")
        .filter_map(|node| {
            node.source_path
                .as_ref()
                .map(|path| (path.clone(), node.id.clone()))
        })
        .collect::<HashMap<_, _>>();
    let mut edge_index = edges
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            (
                edge_key(&edge.source, &edge.target, &edge.relation, &edge.evidence),
                index,
            )
        })
        .collect::<HashMap<_, _>>();

    for task in tasks {
        let task_traces = traces
            .iter()
            .filter(|trace| {
                trace.task_id.as_deref() == Some(task.id.as_str()) || trace.run_id == task.id
            })
            .collect::<Vec<_>>();
        let Some((routine, routine_evidence)) =
            match_task_to_routine(&task, routines, &task_traces)
        else {
            continue;
        };
        let routine_id = format!("routine:{}", routine.id);

        for trace in task_traces {
            if trace.kind == "context" {
                for path in trace
                    .detail
                    .get("injected")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if let Some(memory_id) = memory_by_path.get(path) {
                        merge_edge(
                            edges,
                            &mut edge_index,
                            &routine_id,
                            memory_id,
                            "consulted",
                            routine_evidence,
                            OrbitProvenance {
                                kind: "trace".to_string(),
                                reference: format!("audit:{}", trace.run_id),
                                detail: format!("Memory injected into task {}", task.id),
                                ts: Some(trace.ts.clone()),
                            },
                        );
                    }
                }
            }

            if let Some(path) = trace.detail.get("vaultPath").and_then(Value::as_str) {
                if let Some(memory_id) = memory_by_path.get(path) {
                    merge_edge(
                        edges,
                        &mut edge_index,
                        &routine_id,
                        memory_id,
                        "produced",
                        routine_evidence,
                        OrbitProvenance {
                            kind: "trace".to_string(),
                            reference: format!("audit:{}", trace.run_id),
                            detail: trace.summary.clone(),
                            ts: Some(trace.ts.clone()),
                        },
                    );
                }
            }

            if trace.kind == "tool_call" {
                let trace_text = format!(
                    "{} {}",
                    trace.summary,
                    serde_json::to_string(&trace.detail).unwrap_or_default()
                )
                .to_lowercase();
                for (relation, kind, items) in [
                    ("executed", "skill", skills),
                    ("used", "application", applications),
                ] {
                    for item in items {
                        if let Some(evidence) = catalog_trace_match(item, &trace_text) {
                            let combined_evidence =
                                if routine_evidence == "observed" && evidence == "observed" {
                                    "observed"
                                } else {
                                    "inferred"
                                };
                            merge_edge(
                                edges,
                                &mut edge_index,
                                &routine_id,
                                &format!("{kind}:{}", item.id),
                                relation,
                                combined_evidence,
                                OrbitProvenance {
                                    kind: "trace".to_string(),
                                    reference: format!("audit:{}", trace.run_id),
                                    detail: trace.summary.clone(),
                                    ts: Some(trace.ts.clone()),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    Ok((tasks_scanned, traces_scanned))
}

fn load_tasks(db: &Db) -> AppResult<Vec<TaskRecord>> {
    db.with_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT id, title, goal FROM tasks ORDER BY updated_at DESC LIMIT 500")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TaskRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    goal: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn load_traces(db: &Db) -> AppResult<Vec<TraceRecord>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT run_id, task_id, ts, kind, summary, detail
             FROM audit
             WHERE kind IN ('context', 'tool_call', 'output')
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![5_000i64], |row| {
                let detail: String = row.get(5)?;
                Ok(TraceRecord {
                    run_id: row.get(0)?,
                    task_id: row.get(1)?,
                    ts: row.get(2)?,
                    kind: row.get(3)?,
                    summary: row.get(4)?,
                    detail: serde_json::from_str(&detail).unwrap_or(Value::Null),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn match_task_to_routine<'a>(
    task: &TaskRecord,
    routines: &'a [CatalogItem],
    traces: &[&TraceRecord],
) -> Option<(&'a CatalogItem, &'static str)> {
    let trace_text = traces
        .iter()
        .map(|trace| {
            format!(
                "{} {}",
                trace.summary,
                serde_json::to_string(&trace.detail).unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if let Some(routine) = routines.iter().find(|routine| {
        [
            routine.path.as_str(),
            routine.entrypoint.as_deref().unwrap_or(""),
        ]
        .iter()
        .filter(|candidate| candidate.chars().count() >= 4)
        .any(|candidate| trace_text.contains(&candidate.to_lowercase()))
    }) {
        return Some((routine, "observed"));
    }

    let haystack = stable_key(&format!("{} {}", task.title, task.goal));
    routines
        .iter()
        .filter_map(|routine| {
            let name = stable_key(&routine.display_name);
            let raw_name = stable_key(&routine.name);
            let matched = name.chars().count() >= 4 && haystack.contains(&name)
                || raw_name.chars().count() >= 4 && haystack.contains(&raw_name);
            matched.then_some((routine, name.len().max(raw_name.len())))
        })
        .max_by_key(|(_, score)| *score)
        .map(|(routine, _)| (routine, "inferred"))
}

fn catalog_trace_match(item: &CatalogItem, trace_text: &str) -> Option<&'static str> {
    let exact_candidates = [item.path.as_str(), item.entrypoint.as_deref().unwrap_or("")];
    if exact_candidates
        .iter()
        .filter(|candidate| candidate.chars().count() >= 4)
        .any(|candidate| trace_text.contains(&candidate.to_lowercase()))
    {
        return Some("observed");
    }

    let names = [stable_key(&item.name), stable_key(&item.display_name)];
    let normalized_trace = stable_key(trace_text);
    names
        .iter()
        .any(|name| name.chars().count() >= 5 && normalized_trace.contains(name))
        .then_some("inferred")
}

fn merge_edge(
    edges: &mut Vec<OrbitEdge>,
    edge_index: &mut HashMap<String, usize>,
    source: &str,
    target: &str,
    relation: &str,
    evidence: &str,
    provenance: OrbitProvenance,
) {
    let key = edge_key(source, target, relation, evidence);
    if let Some(index) = edge_index.get(&key).copied() {
        let edge = &mut edges[index];
        edge.weight += 1;
        edge.activity_at = provenance.ts.clone().or_else(|| edge.activity_at.clone());
        if edge.provenance.len() < 8 {
            edge.provenance.push(provenance);
        }
        return;
    }

    let id = format!("edge:{}", stable_key(&key));
    let activity_at = provenance.ts.clone();
    edge_index.insert(key, edges.len());
    edges.push(OrbitEdge {
        id,
        source: source.to_string(),
        target: target.to_string(),
        relation: relation.to_string(),
        evidence: evidence.to_string(),
        weight: 1,
        activity_at,
        provenance: vec![provenance],
    });
}

fn declared_edge(
    source: &str,
    target: &str,
    relation: &str,
    reference: &str,
    detail: &str,
) -> OrbitEdge {
    OrbitEdge {
        id: format!(
            "edge:{}",
            stable_key(&format!("{source}:{target}:{relation}:declared"))
        ),
        source: source.to_string(),
        target: target.to_string(),
        relation: relation.to_string(),
        evidence: "declared".to_string(),
        weight: 1,
        activity_at: None,
        provenance: vec![OrbitProvenance {
            kind: "registry".to_string(),
            reference: reference.to_string(),
            detail: detail.to_string(),
            ts: None,
        }],
    }
}

fn edge_key(source: &str, target: &str, relation: &str, evidence: &str) -> String {
    format!("{source}|{target}|{relation}|{evidence}")
}

fn stable_key(value: &str) -> String {
    let mut result = String::new();
    let mut previous_dash = false;
    for character in value.to_lowercase().chars() {
        if character.is_alphanumeric() {
            result.push(character);
            previous_dash = false;
        } else if !previous_dash && !result.is_empty() {
            result.push('-');
            previous_dash = true;
        }
    }
    result.trim_matches('-').to_string()
}

fn plural_label(kind: &str) -> &'static str {
    match kind {
        "skill" => "skills",
        "routine" => "routines",
        _ => "applications",
    }
}

fn domain_label(domain: &str) -> &'static str {
    match domain {
        "work" => "Work",
        "planphysique" => "PlanPhysique",
        "personal" => "Personal",
        "family" => "Family",
        "finance" => "Finance",
        "research" => "Research",
        _ => "Memory",
    }
}
