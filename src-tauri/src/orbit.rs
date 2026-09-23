use std::collections::{BTreeMap, HashMap, HashSet};

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
    pub activity_window: String,
    pub nodes: Vec<OrbitNode>,
    pub edges: Vec<OrbitEdge>,
    pub activities: Vec<OrbitActivity>,
    pub counts: OrbitCounts,
    pub metrics: OrbitMetrics,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbitMetrics {
    pub compose_ms: f64,
    pub tasks_scanned: usize,
    pub traces_scanned: usize,
    pub activity_events: usize,
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
    pub operational_state: String,
    pub catalog_state: String,
    pub usage_state: String,
    pub connection_state: String,
    pub domains: Vec<OrbitFacet>,
    pub capabilities: Vec<OrbitFacet>,
    pub last_activity_at: Option<String>,
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
pub struct OrbitFacet {
    pub value: String,
    pub evidence: String,
    pub source_ref: String,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbitActivity {
    pub task_id: String,
    pub title: String,
    pub domain: String,
    pub status: String,
    pub updated_at: String,
    pub event_count: usize,
    pub telemetry_available: bool,
    pub links: Vec<OrbitActivityLink>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbitActivityLink {
    pub node_id: String,
    pub relation: String,
    pub event_ref: String,
    pub detail: String,
    pub occurred_at: String,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone)]
struct TaskRecord {
    id: String,
    title: String,
    goal: String,
    domain: String,
    status: String,
    origin_kind: String,
    ontology_category_id: Option<String>,
    updated_at: String,
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

#[derive(Debug, Clone)]
struct ExecutionRef {
    catalog_id: String,
    operation: String,
    outcome: String,
    occurred_at: String,
}

/// Compose the operational map from the authoritative local registries.
/// Filtering happens here, before node counts, previews, and trace relations
/// are produced, so hidden domains or sensitive memories cannot leak through
/// aggregate metadata.
pub fn build(
    db: &Db,
    domain: Option<&str>,
    include_sensitive: bool,
    activity_window: Option<&str>,
) -> AppResult<OrbitMap> {
    let compose_started = std::time::Instant::now();
    if domain.is_some_and(|value| !DOMAINS.contains(&value)) {
        return Err(AppError::Io(std::io::Error::other(
            "invalid orbit domain filter",
        )));
    }
    let activity_window = activity_window.unwrap_or("today");
    if !matches!(activity_window, "today" | "7d") {
        return Err(AppError::Io(std::io::Error::other(
            "invalid orbit activity window",
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
        operational_state: "ready".to_string(),
        catalog_state: "not_applicable".to_string(),
        usage_state: "not_applicable".to_string(),
        connection_state: "not_applicable".to_string(),
        domains: Vec::new(),
        capabilities: vec![
            facet("local_control_plane", "declared", "runtime:agentic-os"),
            facet("governed_execution", "declared", "runtime:agentic-os"),
        ],
        last_activity_at: None,
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
        .filter(|item| catalog_visible_in_domain(item, domain))
        .cloned()
        .collect::<Vec<_>>();
    let routine_items = catalog
        .iter()
        .filter(|item| matches!(item.kind, CatalogKind::Routine | CatalogKind::Workflow))
        .filter(|item| catalog_visible_in_domain(item, domain))
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
        .filter(|item| catalog_visible_in_domain(item, domain))
        .cloned()
        .collect::<Vec<_>>();

    add_catalog_ring(&mut nodes, &mut edges, &skill_items, 1, "skill");
    add_memory_ring(&mut nodes, &mut edges, &memories, domain);
    if include_sensitive {
        let imports = crate::memory::importer::list(db, domain)?;
        add_source_nodes(&mut nodes, &mut edges, &imports);
    }
    add_catalog_ring(&mut nodes, &mut edges, &routine_items, 3, "routine");
    add_catalog_ring(&mut nodes, &mut edges, &application_items, 4, "application");
    add_temporal_relations(&nodes, &mut edges);

    let tasks = load_tasks(db)?
        .into_iter()
        .filter(|task| domain.map_or(true, |value| task.domain == value))
        .collect::<Vec<_>>();
    let task_ids = tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect::<HashSet<_>>();
    let traces = load_traces(db)?
        .into_iter()
        .filter(|trace| {
            domain.is_none()
                || trace.task_id.as_deref().map_or_else(
                    || task_ids.contains(trace.run_id.as_str()),
                    |id| task_ids.contains(id),
                )
        })
        .collect::<Vec<_>>();
    enrich_operational_state(&mut nodes, &tasks, &traces, &routine_items);
    let (tasks_scanned, traces_scanned) = add_observed_relations(
        &nodes,
        &routine_items,
        &skill_items,
        &application_items,
        &tasks,
        &traces,
        &mut edges,
    )?;
    let activities = build_activities(&nodes, &tasks, &traces, activity_window)?;
    let activity_events = activities.iter().map(|activity| activity.event_count).sum();

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
        activity_window: activity_window.to_string(),
        nodes,
        edges,
        activities,
        counts,
        metrics: OrbitMetrics {
            compose_ms: compose_started.elapsed().as_secs_f64() * 1000.0,
            tasks_scanned,
            traces_scanned,
            activity_events,
        },
    })
}

fn add_temporal_relations(nodes: &[OrbitNode], edges: &mut Vec<OrbitEdge>) {
    let memory_by_path = nodes
        .iter()
        .filter(|node| matches!(node.kind.as_str(), "memory" | "source"))
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
        for related in frontmatter.related {
            if let Some(target) = memory_by_path.get(related.as_str()) {
                if *target != node.id {
                    edges.push(declared_edge(
                        &node.id,
                        target,
                        "related_to",
                        path,
                        &format!("Related memory declared in Markdown frontmatter: {related}"),
                    ));
                }
            }
        }
    }
}

fn add_source_nodes(
    nodes: &mut Vec<OrbitNode>,
    edges: &mut Vec<OrbitEdge>,
    imports: &[crate::memory::DocumentImportRecord],
) {
    for import in imports {
        let domain_id = format!("memory-domain:{}", import.domain);
        if !nodes.iter().any(|node| node.id == domain_id) {
            continue;
        }
        let node_id = format!("source:{}", import.id);
        nodes.push(OrbitNode {
            id: node_id.clone(),
            kind: "source".to_string(),
            ring: 2,
            label: import.title.clone(),
            subtitle: Some(format!("{} source", import.input_kind)),
            domain: Some(import.domain.clone()),
            // Imported sources can contain arbitrary document text. Until the
            // import contract carries its own sensitivity classification, the
            // map exposes their metadata only after the explicit sensitive
            // content opt-in and never includes their body as a preview.
            sensitivity: Some("sensitive".to_string()),
            status: import.status.clone(),
            operational_state: import.status.clone(),
            catalog_state: "not_applicable".to_string(),
            usage_state: "not_applicable".to_string(),
            connection_state: "not_applicable".to_string(),
            domains: vec![facet(
                &import.domain,
                "declared",
                &format!("document-import:{}", import.id),
            )],
            capabilities: vec![facet(
                "original_source",
                "declared",
                &format!("document-import:{}", import.id),
            )],
            last_activity_at: Some(import.updated_at.clone()),
            source_path: Some(import.source_path.clone()),
            source_ref: format!("document-import:{}", import.id),
            group_id: Some(domain_id.clone()),
            count: 1,
            preview: None,
            updated_at: Some(import.updated_at.clone()),
            actions: vec!["open_source".to_string()],
            aggregate: false,
        });
        edges.push(declared_edge(
            &domain_id,
            &node_id,
            "contains_source",
            &format!("document-import:{}", import.id),
            "Original source metadata from the governed import registry",
        ));
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
            operational_state: "available".to_string(),
            catalog_state: "registered".to_string(),
            usage_state: "not_observed".to_string(),
            connection_state: if kind == "application" {
                "unknown"
            } else {
                "not_applicable"
            }
            .to_string(),
            domains: aggregate_catalog_domains(&group_items),
            capabilities: aggregate_catalog_capabilities(&group_items, kind),
            last_activity_at: group_items
                .iter()
                .filter_map(|item| item.updated_at)
                .max()
                .and_then(catalog_timestamp),
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
                operational_state: "available".to_string(),
                catalog_state: "registered".to_string(),
                usage_state: "not_observed".to_string(),
                connection_state: if kind == "application" {
                    "unknown"
                } else {
                    "not_applicable"
                }
                .to_string(),
                domains: catalog_domains(item),
                capabilities: catalog_capabilities(item, kind),
                last_activity_at: item.updated_at.and_then(catalog_timestamp),
                source_path: Some(item.path.clone()),
                source_ref: format!("catalog:{}", item.id),
                group_id: Some(group_id.clone()),
                count: 1,
                preview: item.summary.clone(),
                updated_at: item.updated_at.and_then(catalog_timestamp),
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
            operational_state: if domain_memories
                .iter()
                .any(|memory| memory.status == "stale")
            {
                "attention".to_string()
            } else {
                "ready".to_string()
            },
            catalog_state: "not_applicable".to_string(),
            usage_state: "not_applicable".to_string(),
            connection_state: "not_applicable".to_string(),
            domains: vec![facet(domain, "declared", &format!("vault-domain:{domain}"))],
            capabilities: vec![facet(
                "governed_memory",
                "declared",
                &format!("vault-domain:{domain}"),
            )],
            last_activity_at: domain_memories
                .iter()
                .map(|memory| memory.updated_at.as_str())
                .max()
                .map(str::to_string),
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
                operational_state: memory.status.clone(),
                catalog_state: "not_applicable".to_string(),
                usage_state: "not_applicable".to_string(),
                connection_state: "not_applicable".to_string(),
                domains: vec![facet(&memory.domain, "declared", &memory.vault_path)],
                capabilities: vec![facet(
                    &format!("memory:{}", memory.mem_type),
                    "declared",
                    &memory.vault_path,
                )],
                last_activity_at: Some(memory.updated_at.clone()),
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
    nodes: &[OrbitNode],
    routines: &[CatalogItem],
    skills: &[CatalogItem],
    applications: &[CatalogItem],
    tasks: &[TaskRecord],
    traces: &[TraceRecord],
    edges: &mut Vec<OrbitEdge>,
) -> AppResult<(usize, usize)> {
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

    // Executor-qualified events are observable even when the task cannot be
    // attributed to a catalog routine. Keep that fact as a direct AgenticOS
    // relation; routine-specific relations below still require both endpoints
    // to be identified independently.
    for trace in traces.iter().filter(|trace| trace.kind == "tool_call") {
        for (relation, kind, items) in [
            ("executed", "skill", skills),
            ("executed", "routine", routines),
            ("used", "application", applications),
        ] {
            for reference in execution_refs(&trace.detail, kind) {
                let Some(item) = items.iter().find(|item| item.id == reference.catalog_id) else {
                    continue;
                };
                let target = format!("{kind}:{}", item.id);
                if !nodes.iter().any(|node| node.id == target) {
                    continue;
                }
                merge_edge(
                    edges,
                    &mut edge_index,
                    "core:agentic-os",
                    &target,
                    relation,
                    "observed",
                    OrbitProvenance {
                        kind: "execution_event".to_string(),
                        reference: format!("audit:{}", trace.run_id),
                        detail: format!(
                            "{}: {} ({})",
                            reference.operation, item.display_name, reference.outcome
                        ),
                        ts: Some(reference.occurred_at),
                    },
                );
            }
        }
    }

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
                for memory_id in structured_memory_ids(&trace.detail) {
                    let target = format!("memory:{memory_id}");
                    if nodes.iter().any(|node| node.id == target) {
                        merge_edge(
                            edges,
                            &mut edge_index,
                            &routine_id,
                            &target,
                            "consulted",
                            if routine_evidence == "observed" {
                                "observed"
                            } else {
                                "inferred"
                            },
                            OrbitProvenance {
                                kind: "structured_event".to_string(),
                                reference: format!("audit:{}", trace.run_id),
                                detail: format!(
                                    "Memory id persisted in task {} context event",
                                    task.id
                                ),
                                ts: Some(trace.ts.clone()),
                            },
                        );
                    }
                }
                for path in trace
                    .detail
                    .get("injected")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if let Some(memory_id) = memory_by_path.get(path) {
                        if structured_memory_ids(&trace.detail)
                            .iter()
                            .any(|id| memory_id == &format!("memory:{id}"))
                        {
                            continue;
                        }
                        merge_edge(
                            edges,
                            &mut edge_index,
                            &routine_id,
                            memory_id,
                            "consulted",
                            "inferred",
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

            if let Some(memory_id) = trace.detail.get("memoryId").and_then(Value::as_str) {
                let target = format!("memory:{memory_id}");
                if nodes.iter().any(|node| node.id == target) {
                    merge_edge(
                        edges,
                        &mut edge_index,
                        &routine_id,
                        &target,
                        "produced",
                        if routine_evidence == "observed" {
                            "observed"
                        } else {
                            "inferred"
                        },
                        OrbitProvenance {
                            kind: "structured_event".to_string(),
                            reference: format!("audit:{}", trace.run_id),
                            detail: "Structured memory reference recorded in task context"
                                .to_string(),
                            ts: Some(trace.ts.clone()),
                        },
                    );
                }
            } else if let Some(path) = trace.detail.get("vaultPath").and_then(Value::as_str) {
                if let Some(memory_id) = memory_by_path.get(path) {
                    merge_edge(
                        edges,
                        &mut edge_index,
                        &routine_id,
                        memory_id,
                        "produced",
                        "inferred",
                        OrbitProvenance {
                            kind: "trace".to_string(),
                            reference: format!("audit:{}", trace.run_id),
                            detail: "Vault path matched a visible memory output".to_string(),
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
                    let execution_refs = execution_refs(&trace.detail, kind);
                    let inferred_ids = inferred_catalog_ids(&trace.detail, kind);
                    for item in items {
                        let execution = execution_refs
                            .iter()
                            .find(|reference| reference.catalog_id == item.id);
                        let inferred = inferred_ids.iter().any(|id| id == &item.id)
                            || catalog_trace_match(item, &trace_text).is_some();
                        if execution.is_some() || inferred {
                            let evidence = if execution.is_some() {
                                "observed"
                            } else {
                                "inferred"
                            };
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
                                    kind: if execution.is_some() {
                                        "execution_event"
                                    } else {
                                        "trace_inference"
                                    }
                                    .to_string(),
                                    reference: format!("audit:{}", trace.run_id),
                                    detail: execution.map_or_else(
                                        || {
                                            "Catalog reference inferred from trace metadata"
                                                .to_string()
                                        },
                                        |reference| {
                                            format!(
                                                "{}: {} ({})",
                                                reference.operation,
                                                item.display_name,
                                                reference.outcome
                                            )
                                        },
                                    ),
                                    ts: execution
                                        .map(|reference| reference.occurred_at.clone())
                                        .or_else(|| Some(trace.ts.clone())),
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

fn activity_since(window: &str) -> chrono::DateTime<chrono::Utc> {
    let now = chrono::Utc::now();
    if window == "7d" {
        now - chrono::Duration::days(7)
    } else {
        now.date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time")
            .and_utc()
    }
}

fn timestamp_at_or_after(value: &str, since: chrono::DateTime<chrono::Utc>) -> bool {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&chrono::Utc) >= since)
        .unwrap_or(false)
}

fn build_activities(
    nodes: &[OrbitNode],
    tasks: &[TaskRecord],
    traces: &[TraceRecord],
    window: &str,
) -> AppResult<Vec<OrbitActivity>> {
    build_activities_since(nodes, tasks, traces, activity_since(window))
}

fn build_activities_since(
    nodes: &[OrbitNode],
    tasks: &[TaskRecord],
    traces: &[TraceRecord],
    since: chrono::DateTime<chrono::Utc>,
) -> AppResult<Vec<OrbitActivity>> {
    let visible_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();
    let mut activities = Vec::new();

    for task in tasks {
        let task_traces = traces
            .iter()
            .filter(|trace| {
                (trace.task_id.as_deref() == Some(task.id.as_str()) || trace.run_id == task.id)
                    && timestamp_at_or_after(&trace.ts, since)
            })
            .collect::<Vec<_>>();
        if task_traces.is_empty() && !timestamp_at_or_after(&task.updated_at, since) {
            continue;
        }

        let mut links = Vec::new();
        let mut seen = HashSet::new();
        for trace in &task_traces {
            for (kind, relation) in [
                ("skill", "executed"),
                ("routine", "executed"),
                ("application", "used"),
            ] {
                for reference in execution_refs(&trace.detail, kind) {
                    let node_id = format!("{kind}:{}", reference.catalog_id);
                    if !visible_ids.contains(node_id.as_str()) {
                        continue;
                    }
                    let key = format!(
                        "{node_id}|{relation}|{}|{}",
                        trace.run_id, reference.occurred_at
                    );
                    if !seen.insert(key) {
                        continue;
                    }
                    links.push(OrbitActivityLink {
                        node_id,
                        relation: relation.to_string(),
                        event_ref: format!("audit:{}", trace.run_id),
                        detail: format!("{} ({})", reference.operation, reference.outcome),
                        occurred_at: reference.occurred_at,
                        outcome: Some(reference.outcome),
                    });
                }
            }

            if trace.kind == "context" {
                for memory_id in structured_memory_ids(&trace.detail) {
                    let node_id = format!("memory:{memory_id}");
                    if visible_ids.contains(node_id.as_str())
                        && seen.insert(format!(
                            "{node_id}|inserted_into_context|{}|{}",
                            trace.run_id, trace.ts
                        ))
                    {
                        links.push(OrbitActivityLink {
                            node_id,
                            relation: "inserted_into_context".to_string(),
                            event_ref: format!("audit:{}", trace.run_id),
                            detail: "Memory identifier recorded in the task context event"
                                .to_string(),
                            occurred_at: trace.ts.clone(),
                            outcome: None,
                        });
                    }
                }
            }

            if let Some(memory_id) = trace.detail.get("memoryId").and_then(Value::as_str) {
                let node_id = format!("memory:{memory_id}");
                if visible_ids.contains(node_id.as_str())
                    && seen.insert(format!("{node_id}|produced|{}|{}", trace.run_id, trace.ts))
                {
                    links.push(OrbitActivityLink {
                        node_id,
                        relation: "produced".to_string(),
                        event_ref: format!("audit:{}", trace.run_id),
                        detail: "Memory identifier recorded in the task output event".to_string(),
                        occurred_at: trace.ts.clone(),
                        outcome: None,
                    });
                }
            }
        }

        let updated_at = task_traces
            .iter()
            .map(|trace| trace.ts.as_str())
            .max()
            .unwrap_or(task.updated_at.as_str())
            .to_string();
        activities.push(OrbitActivity {
            task_id: task.id.clone(),
            title: task.title.clone(),
            domain: task.domain.clone(),
            status: task.status.clone(),
            updated_at,
            event_count: task_traces.len(),
            telemetry_available: !task_traces.is_empty(),
            links,
        });
    }

    activities.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    activities.truncate(100);
    Ok(activities)
}

fn load_tasks(db: &Db) -> AppResult<Vec<TaskRecord>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, goal, domain, status, origin_kind, ontology_category_id, updated_at
             FROM tasks ORDER BY updated_at DESC LIMIT 500",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TaskRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    goal: row.get(2)?,
                    domain: row.get(3)?,
                    status: row.get(4)?,
                    origin_kind: row.get(5)?,
                    ontology_category_id: row.get(6)?,
                    updated_at: row.get(7)?,
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

fn inferred_catalog_ids(detail: &Value, kind: &str) -> Vec<String> {
    detail
        .get("catalogRefs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|reference| reference.get("kind").and_then(Value::as_str) == Some(kind))
        .filter_map(|reference| reference.get("catalogId").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// Only executor-emitted references with the complete observation envelope
/// may become `observed` graph evidence. `catalogRefs` are deliberately not
/// accepted here: they are lookup hints derived from names, paths or commands.
fn execution_refs(detail: &Value, kind: &str) -> Vec<ExecutionRef> {
    detail
        .get("executionRefs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|reference| reference.get("kind").and_then(Value::as_str) == Some(kind))
        .filter_map(|reference| {
            let catalog_id = reference.get("catalogId")?.as_str()?.trim();
            let operation = reference.get("operation")?.as_str()?.trim();
            let outcome = reference.get("outcome")?.as_str()?.trim();
            let occurred_at = reference.get("occurredAt")?.as_str()?.trim();
            if catalog_id.is_empty()
                || operation.is_empty()
                || outcome.is_empty()
                || chrono::DateTime::parse_from_rfc3339(occurred_at).is_err()
            {
                return None;
            }
            Some(ExecutionRef {
                catalog_id: catalog_id.to_string(),
                operation: operation.to_string(),
                outcome: outcome.to_string(),
                occurred_at: occurred_at.to_string(),
            })
        })
        .collect()
}

fn structured_memory_ids(detail: &Value) -> Vec<String> {
    detail
        .get("memoryRefs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|reference| reference.get("memoryId").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn enrich_operational_state(
    nodes: &mut [OrbitNode],
    tasks: &[TaskRecord],
    traces: &[TraceRecord],
    routines: &[CatalogItem],
) {
    if let Some(core) = nodes.iter_mut().find(|node| node.id == "core:agentic-os") {
        core.operational_state = if tasks.iter().any(|task| task_is_active(&task.status)) {
            "running"
        } else if tasks
            .iter()
            .any(|task| matches!(task.status.as_str(), "failed" | "waiting_for_approval"))
        {
            "attention"
        } else {
            "ready"
        }
        .to_string();
        core.last_activity_at = tasks
            .iter()
            .map(|task| task.updated_at.as_str())
            .max()
            .map(str::to_string);
    }

    // Execution history owns usage, last activity, and connector health. It is
    // reduced independently of task state so a completed/failed historical
    // task cannot masquerade as current activity. Audit rows are currently
    // loaded newest-first, but correctness must not depend on query order.
    let mut latest_connection_at = HashMap::<String, chrono::DateTime<chrono::FixedOffset>>::new();
    for trace in traces {
        for kind in ["skill", "application"] {
            for reference in execution_refs(&trace.detail, kind) {
                let node_id = format!("{kind}:{}", reference.catalog_id);
                let Some(node) = nodes.iter_mut().find(|node| node.id == node_id) else {
                    continue;
                };
                node.usage_state = "observed".to_string();
                if timestamp_is_newer(&reference.occurred_at, node.last_activity_at.as_deref()) {
                    node.last_activity_at = Some(reference.occurred_at.clone());
                }
                if kind == "application" {
                    let occurred_at = chrono::DateTime::parse_from_rfc3339(&reference.occurred_at)
                        .expect("execution_refs validates RFC 3339 timestamps");
                    let is_latest = latest_connection_at
                        .get(&node_id)
                        .map_or(true, |latest| occurred_at > *latest);
                    if is_latest {
                        node.connection_state = connection_state(&reference.outcome).to_string();
                        latest_connection_at.insert(node_id, occurred_at);
                    }
                }
            }
        }
    }

    for task in tasks {
        let task_traces = traces
            .iter()
            .filter(|trace| {
                trace.task_id.as_deref() == Some(task.id.as_str()) || trace.run_id == task.id
            })
            .collect::<Vec<_>>();
        if let Some((routine, evidence)) = match_task_to_routine(task, routines, &task_traces) {
            if let Some(node) = nodes
                .iter_mut()
                .find(|node| node.id == format!("routine:{}", routine.id))
            {
                apply_task_state(node, task, evidence);
            }
        }

        for trace in task_traces {
            for kind in ["skill", "application"] {
                let observed_refs = execution_refs(&trace.detail, kind);
                for reference in &observed_refs {
                    if let Some(node) = nodes
                        .iter_mut()
                        .find(|node| node.id == format!("{kind}:{}", reference.catalog_id))
                    {
                        if task_is_active(&task.status) {
                            node.operational_state = "in_use".to_string();
                        }
                        push_facet(
                            &mut node.domains,
                            facet(&task.domain, "observed", &format!("task:{}", task.id)),
                        );
                    }
                }
                for catalog_id in inferred_catalog_ids(&trace.detail, kind) {
                    if observed_refs
                        .iter()
                        .any(|reference| reference.catalog_id == catalog_id)
                    {
                        continue;
                    }
                    if let Some(node) = nodes
                        .iter_mut()
                        .find(|node| node.id == format!("{kind}:{catalog_id}"))
                    {
                        if node.usage_state != "observed" {
                            node.usage_state = "inferred".to_string();
                        }
                        push_facet(
                            &mut node.domains,
                            facet(&task.domain, "inferred", &format!("task:{}", task.id)),
                        );
                    }
                }
            }
        }
    }
    roll_up_catalog_states(nodes);
}

fn roll_up_catalog_states(nodes: &mut [OrbitNode]) {
    let mut child_states: HashMap<String, Vec<(String, String, String)>> = HashMap::new();
    for node in nodes.iter().filter(|node| !node.aggregate) {
        if let Some(group_id) = &node.group_id {
            child_states.entry(group_id.clone()).or_default().push((
                node.operational_state.clone(),
                node.usage_state.clone(),
                node.connection_state.clone(),
            ));
        }
    }
    for node in nodes
        .iter_mut()
        .filter(|node| node.aggregate && node.catalog_state == "registered")
    {
        let Some(states) = child_states.get(&node.id) else {
            continue;
        };
        node.usage_state = if states.iter().any(|state| state.1 == "observed") {
            "observed"
        } else if states.iter().any(|state| state.1 == "inferred") {
            "inferred"
        } else {
            "not_observed"
        }
        .to_string();
        if node.kind == "application_group" {
            node.connection_state = if states.iter().any(|state| state.2 == "failing") {
                "failing"
            } else if states.iter().any(|state| state.2 == "working") {
                "working"
            } else {
                "unknown"
            }
            .to_string();
        }
        if states.iter().any(|state| state.0 == "attention") {
            node.operational_state = "attention".to_string();
        } else if states
            .iter()
            .any(|state| matches!(state.0.as_str(), "running" | "in_use"))
        {
            node.operational_state = "in_use".to_string();
        }
    }
}

fn task_is_active(status: &str) -> bool {
    matches!(
        status,
        "planned" | "running" | "waiting_for_tool" | "resuming" | "verifying"
    )
}

fn timestamp_is_newer(candidate: &str, current: Option<&str>) -> bool {
    let Ok(candidate) = chrono::DateTime::parse_from_rfc3339(candidate) else {
        return false;
    };
    match current.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok()) {
        Some(current) => candidate > current,
        None => true,
    }
}

fn apply_task_state(node: &mut OrbitNode, task: &TaskRecord, evidence: &str) {
    if node.usage_state != "observed" || evidence == "observed" {
        node.usage_state = evidence.to_string();
    }
    if evidence == "observed"
        && node.last_activity_at.as_deref().unwrap_or("") <= task.updated_at.as_str()
    {
        node.operational_state = task.status.clone();
        node.last_activity_at = Some(task.updated_at.clone());
    }
    push_facet(
        &mut node.domains,
        facet(&task.domain, evidence, &format!("task:{}", task.id)),
    );
    push_facet(
        &mut node.capabilities,
        facet(
            &format!("origin:{}", task.origin_kind),
            evidence,
            &format!("task:{}", task.id),
        ),
    );
    if let Some(category) = task.ontology_category_id.as_deref() {
        push_facet(
            &mut node.capabilities,
            facet(
                &format!("ontology:{category}"),
                evidence,
                &format!("task:{}", task.id),
            ),
        );
    }
}

fn push_facet(facets: &mut Vec<OrbitFacet>, candidate: OrbitFacet) {
    if !facets.iter().any(|existing| {
        existing.value == candidate.value && existing.evidence == candidate.evidence
    }) {
        facets.push(candidate);
    }
}

fn match_task_to_routine<'a>(
    task: &TaskRecord,
    routines: &'a [CatalogItem],
    traces: &[&TraceRecord],
) -> Option<(&'a CatalogItem, &'static str)> {
    for trace in traces {
        for reference in execution_refs(&trace.detail, "routine") {
            if let Some(routine) = routines
                .iter()
                .find(|routine| routine.id == reference.catalog_id)
            {
                return Some((routine, "observed"));
            }
        }
    }
    for trace in traces {
        if let Some(id) = trace
            .detail
            .get("routineId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                inferred_catalog_ids(&trace.detail, "routine")
                    .into_iter()
                    .next()
            })
        {
            if let Some(routine) = routines.iter().find(|routine| routine.id == id) {
                return Some((routine, "inferred"));
            }
        }
    }
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
        return Some((routine, "inferred"));
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

fn connection_state(outcome: &str) -> &'static str {
    match outcome.to_ascii_lowercase().as_str() {
        "success" | "succeeded" | "completed" | "ok" => "working",
        "failure" | "failed" | "error" => "failing",
        _ => "unknown",
    }
}

fn catalog_trace_match(item: &CatalogItem, trace_text: &str) -> Option<&'static str> {
    let exact_candidates = [item.path.as_str(), item.entrypoint.as_deref().unwrap_or("")];
    if exact_candidates
        .iter()
        .filter(|candidate| candidate.chars().count() >= 4)
        .any(|candidate| trace_text.contains(&candidate.to_lowercase()))
    {
        return Some("inferred");
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
        if provenance
            .ts
            .as_deref()
            .is_some_and(|candidate| timestamp_is_newer(candidate, edge.activity_at.as_deref()))
        {
            edge.activity_at = provenance.ts.clone();
        }
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

fn facet(value: &str, evidence: &str, source_ref: &str) -> OrbitFacet {
    OrbitFacet {
        value: value.to_string(),
        evidence: evidence.to_string(),
        source_ref: source_ref.to_string(),
    }
}

fn catalog_timestamp(value: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(value).map(|timestamp| timestamp.to_rfc3339())
}

fn catalog_domains(item: &CatalogItem) -> Vec<OrbitFacet> {
    item.tags
        .iter()
        .filter_map(|tag| {
            let value = tag.strip_prefix("domain:").unwrap_or(tag).to_lowercase();
            DOMAINS
                .contains(&value.as_str())
                .then(|| facet(&value, "declared", &format!("catalog:{}", item.id)))
        })
        .collect()
}

fn catalog_visible_in_domain(item: &CatalogItem, domain: Option<&str>) -> bool {
    let Some(domain) = domain else {
        return true;
    };
    let declared = catalog_domains(item);
    declared.is_empty() || declared.iter().any(|entry| entry.value == domain)
}

fn catalog_capabilities(item: &CatalogItem, kind: &str) -> Vec<OrbitFacet> {
    let source = format!("catalog:{}", item.id);
    let mut values = std::collections::BTreeSet::from([format!("{kind}:{}", item.name)]);
    if let Some(category) = item
        .category
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        values.insert(format!("category:{category}"));
    }
    for tag in &item.tags {
        if let Some(value) = tag.strip_prefix("capability:") {
            values.insert(value.to_string());
        }
    }
    values
        .into_iter()
        .map(|value| facet(&value, "declared", &source))
        .collect()
}

fn aggregate_catalog_domains(items: &[&CatalogItem]) -> Vec<OrbitFacet> {
    let mut values = BTreeMap::new();
    for item in items {
        for domain in catalog_domains(item) {
            values.entry(domain.value.clone()).or_insert(domain);
        }
    }
    values.into_values().collect()
}

fn aggregate_catalog_capabilities(items: &[&CatalogItem], kind: &str) -> Vec<OrbitFacet> {
    let mut values = BTreeMap::new();
    for item in items {
        for capability in catalog_capabilities(item, kind) {
            values.entry(capability.value.clone()).or_insert(capability);
        }
    }
    values.into_values().collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn routine() -> CatalogItem {
        CatalogItem {
            id: "daily-brief".to_string(),
            kind: CatalogKind::Routine,
            name: "daily-brief".to_string(),
            display_name: "Daily Brief".to_string(),
            summary: None,
            path: "/vault/routines/daily-brief.toml".to_string(),
            origin: "workspace".to_string(),
            group: "Briefing".to_string(),
            tags: vec!["domain:work".to_string()],
            version: None,
            category: Some("briefing".to_string()),
            updated_at: None,
            provider: "local".to_string(),
            detector: "test".to_string(),
            entrypoint: Some("run-daily-brief".to_string()),
            confidence: 1.0,
        }
    }

    fn application() -> CatalogItem {
        CatalogItem {
            id: "research-mcp".to_string(),
            kind: CatalogKind::Mcp,
            name: "research-server".to_string(),
            display_name: "Research server".to_string(),
            summary: None,
            path: "/vault/connectors/research.json".to_string(),
            origin: "workspace".to_string(),
            group: "Research".to_string(),
            tags: vec!["domain:research".to_string()],
            version: None,
            category: Some("search".to_string()),
            updated_at: None,
            provider: "local".to_string(),
            detector: "test".to_string(),
            entrypoint: None,
            confidence: 1.0,
        }
    }

    fn core_node() -> OrbitNode {
        OrbitNode {
            id: "core:agentic-os".to_string(),
            kind: "core".to_string(),
            ring: 0,
            label: "AgenticOS".to_string(),
            subtitle: None,
            domain: None,
            sensitivity: None,
            status: "active".to_string(),
            operational_state: "ready".to_string(),
            catalog_state: "not_applicable".to_string(),
            usage_state: "not_applicable".to_string(),
            connection_state: "not_applicable".to_string(),
            domains: Vec::new(),
            capabilities: Vec::new(),
            last_activity_at: None,
            source_path: None,
            source_ref: "runtime:agentic-os".to_string(),
            group_id: None,
            count: 1,
            preview: None,
            updated_at: None,
            actions: Vec::new(),
            aggregate: false,
        }
    }

    fn task() -> TaskRecord {
        TaskRecord {
            id: "task-1".to_string(),
            title: "Unrelated title".to_string(),
            goal: "Unrelated goal".to_string(),
            domain: "work".to_string(),
            status: "completed".to_string(),
            origin_kind: "manual".to_string(),
            ontology_category_id: None,
            updated_at: "2026-09-23T10:00:00Z".to_string(),
        }
    }

    #[test]
    fn text_derived_catalog_reference_remains_inferred() {
        let trace = TraceRecord {
            run_id: "task-1".to_string(),
            task_id: Some("task-1".to_string()),
            ts: "2026-09-23T10:00:00Z".to_string(),
            kind: "tool_call".to_string(),
            summary: "called tool".to_string(),
            detail: serde_json::json!({
                "catalogRefs": [{
                    "catalogId": "daily-brief",
                    "kind": "routine",
                    "evidence": "inferred",
                    "derivation": "command_text"
                }]
            }),
        };
        let routines = vec![routine()];
        let matched = match_task_to_routine(&task(), &routines, &[&trace]).unwrap();
        assert_eq!(matched.0.id, "daily-brief");
        assert_eq!(matched.1, "inferred");
    }

    #[test]
    fn complete_executor_reference_is_observed() {
        let trace = TraceRecord {
            run_id: "task-1".to_string(),
            task_id: Some("task-1".to_string()),
            ts: "2026-09-23T10:00:01Z".to_string(),
            kind: "tool_call".to_string(),
            summary: "executed routine".to_string(),
            detail: serde_json::json!({
                "executionRefs": [{
                    "catalogId": "daily-brief",
                    "kind": "routine",
                    "operation": "execute",
                    "outcome": "succeeded",
                    "occurredAt": "2026-09-23T10:00:00Z"
                }]
            }),
        };
        let routines = vec![routine()];
        let matched = match_task_to_routine(&task(), &routines, &[&trace]).unwrap();
        assert_eq!(matched.0.id, "daily-brief");
        assert_eq!(matched.1, "observed");
    }

    #[test]
    fn incomplete_executor_reference_is_not_observed() {
        let detail = serde_json::json!({
            "executionRefs": [{
                "catalogId": "daily-brief",
                "kind": "routine",
                "outcome": "succeeded",
                "occurredAt": "not-a-timestamp"
            }]
        });
        assert!(execution_refs(&detail, "routine").is_empty());
    }

    fn application_execution_trace(outcome: &str, occurred_at: &str) -> TraceRecord {
        TraceRecord {
            run_id: format!("run-{outcome}-{occurred_at}"),
            task_id: None,
            ts: occurred_at.to_string(),
            kind: "tool_call".to_string(),
            summary: format!("connector {outcome}"),
            detail: serde_json::json!({
                "executionRefs": [{
                    "catalogId": "research-mcp",
                    "kind": "application",
                    "operation": "mcp_tool:search",
                    "outcome": outcome,
                    "occurredAt": occurred_at
                }]
            }),
        }
    }

    #[test]
    fn connector_state_uses_latest_event_independent_of_trace_order() {
        for (older_outcome, newer_outcome, expected_state) in [
            ("failed", "succeeded", "working"),
            ("succeeded", "failed", "failing"),
        ] {
            let chronological = vec![
                application_execution_trace(older_outcome, "2026-09-23T11:00:00+00:00"),
                application_execution_trace(newer_outcome, "2026-09-23T12:00:00+00:00"),
            ];
            for traces in [
                chronological.clone(),
                chronological.iter().rev().cloned().collect::<Vec<_>>(),
            ] {
                let application = application();
                let mut nodes = vec![core_node()];
                let mut edges = Vec::new();
                add_catalog_ring(
                    &mut nodes,
                    &mut edges,
                    std::slice::from_ref(&application),
                    4,
                    "application",
                );

                enrich_operational_state(&mut nodes, &[], &traces, &[]);

                let node = nodes
                    .iter()
                    .find(|node| node.id == "application:research-mcp")
                    .unwrap();
                assert_eq!(node.connection_state, expected_state);
                assert_eq!(node.operational_state, "available");
                assert_eq!(node.usage_state, "observed");
                assert_eq!(
                    node.last_activity_at.as_deref(),
                    Some("2026-09-23T12:00:00+00:00")
                );

                let mut active_traces = traces.clone();
                for trace in &mut active_traces {
                    trace.task_id = Some("task-1".to_string());
                }
                let mut active_task = task();
                active_task.status = "running".to_string();
                enrich_operational_state(&mut nodes, &[active_task], &active_traces, &[]);
                let active_node = nodes
                    .iter()
                    .find(|node| node.id == "application:research-mcp")
                    .unwrap();
                assert_eq!(active_node.operational_state, "in_use");
                assert_eq!(active_node.connection_state, expected_state);
            }
        }
    }

    #[test]
    fn completed_mcp_execution_flows_through_audit_to_an_observed_relation() {
        let db_path = std::env::temp_dir().join(format!(
            "agentic-os-orbit-observed-execution-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Db::open(&db_path).unwrap();
        let application = application();
        let occurred_at = "2026-09-23T10:00:00Z";
        let detail = crate::harness::codex::enrich_structured_refs_with_catalog(
            serde_json::json!({
                "type": "item.completed",
                "item": {
                    "id": "mcp-call-1",
                    "type": "mcp_tool_call",
                    "server": "research-server",
                    "tool": "search",
                    "status": "completed"
                }
            }),
            "tool_call",
            std::slice::from_ref(&application),
            occurred_at,
        );
        crate::audit::append_row(
            &db,
            "run-1",
            "task-1",
            "tool_call",
            "MCP search completed",
            &detail,
            None,
            None,
        )
        .unwrap();
        let persisted = crate::audit::read_trace(&db, "run-1").unwrap();
        assert_eq!(
            persisted[0].detail["executionRefs"][0]["catalogId"],
            "research-mcp"
        );

        let mut nodes = vec![core_node()];
        let mut edges = Vec::new();
        add_catalog_ring(
            &mut nodes,
            &mut edges,
            std::slice::from_ref(&application),
            4,
            "application",
        );
        let traces = load_traces(&db).unwrap();
        add_observed_relations(
            &nodes,
            &[],
            &[],
            std::slice::from_ref(&application),
            &[],
            &traces,
            &mut edges,
        )
        .unwrap();
        assert!(edges.iter().any(|edge| {
            edge.source == "core:agentic-os"
                && edge.target == "application:research-mcp"
                && edge.relation == "used"
                && edge.evidence == "observed"
                && edge.provenance[0].ts.as_deref() == Some(occurred_at)
        }));

        drop(db);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn text_path_match_remains_inferred() {
        let routine = routine();
        let trace_text = format!("executed {}", routine.path);
        assert_eq!(catalog_trace_match(&routine, &trace_text), Some("inferred"));
    }

    fn activity_fixture_nodes() -> Vec<OrbitNode> {
        let mut skill = core_node();
        skill.id = "skill:verified-skill".to_string();
        skill.kind = "skill".to_string();
        skill.ring = 1;
        skill.label = "Verified skill".to_string();
        skill.source_ref = "catalog:verified-skill".to_string();

        let mut memory = core_node();
        memory.id = "memory:decision-1".to_string();
        memory.kind = "memory".to_string();
        memory.ring = 2;
        memory.label = "Decision".to_string();
        memory.domain = Some("work".to_string());
        memory.source_ref = "memory:decision-1".to_string();

        let mut application = core_node();
        application.id = "application:research-mcp".to_string();
        application.kind = "application".to_string();
        application.ring = 4;
        application.label = "Research server".to_string();
        application.source_ref = "catalog:research-mcp".to_string();
        vec![core_node(), skill, memory, application]
    }

    #[test]
    fn activity_uses_only_structured_execution_and_memory_references() {
        let traces = vec![
            TraceRecord {
                run_id: "run-1".to_string(),
                task_id: Some("task-1".to_string()),
                ts: "2026-09-23T10:00:00Z".to_string(),
                kind: "tool_call".to_string(),
                summary: "structured execution".to_string(),
                detail: serde_json::json!({
                    "executionRefs": [
                        {"catalogId":"verified-skill","kind":"skill","operation":"invoke","outcome":"succeeded","occurredAt":"2026-09-23T10:00:00Z"},
                        {"catalogId":"research-mcp","kind":"application","operation":"mcp_tool:search","outcome":"succeeded","occurredAt":"2026-09-23T10:00:00Z"}
                    ],
                    "catalogRefs": [{"catalogId":"unverified","kind":"skill","evidence":"inferred"}]
                }),
            },
            TraceRecord {
                run_id: "run-1".to_string(),
                task_id: Some("task-1".to_string()),
                ts: "2026-09-23T10:00:01Z".to_string(),
                kind: "context".to_string(),
                summary: "context assembled".to_string(),
                detail: serde_json::json!({"memoryRefs":[{"memoryId":"decision-1"}]}),
            },
        ];
        let activities = build_activities_since(
            &activity_fixture_nodes(),
            &[task()],
            &traces,
            chrono::DateTime::parse_from_rfc3339("2026-09-23T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
        .unwrap();
        let activity = &activities[0];
        assert!(activity.telemetry_available);
        assert_eq!(activity.event_count, 2);
        assert!(activity.links.iter().any(|link| {
            link.node_id == "skill:verified-skill"
                && link.relation == "executed"
                && link.event_ref == "audit:run-1"
        }));
        assert!(activity
            .links
            .iter()
            .any(|link| { link.node_id == "application:research-mcp" && link.relation == "used" }));
        assert!(activity.links.iter().any(|link| {
            link.node_id == "memory:decision-1" && link.relation == "inserted_into_context"
        }));
        assert!(activity
            .links
            .iter()
            .all(|link| !link.node_id.contains("unverified")));
    }

    #[test]
    fn text_or_catalog_inference_never_becomes_activity() {
        let trace = TraceRecord {
            run_id: "run-1".to_string(),
            task_id: Some("task-1".to_string()),
            ts: "2026-09-23T10:00:00Z".to_string(),
            kind: "tool_call".to_string(),
            summary: "printed /vault/connectors/research.json".to_string(),
            detail: serde_json::json!({
                "command":"printf /vault/connectors/research.json",
                "catalogRefs":[{"catalogId":"research-mcp","kind":"application","evidence":"inferred"}]
            }),
        };
        let activities = build_activities_since(
            &activity_fixture_nodes(),
            &[task()],
            &[trace],
            chrono::DateTime::parse_from_rfc3339("2026-09-23T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
        .unwrap();
        assert!(activities[0].telemetry_available);
        assert!(activities[0].links.is_empty());
    }

    #[test]
    fn activity_window_rejects_unknown_values() {
        let db_path = std::env::temp_dir().join(format!(
            "agentic-os-orbit-invalid-window-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Db::open(&db_path).unwrap();
        let error = build(&db, None, false, Some("all-time")).unwrap_err();
        assert!(error.to_string().contains("invalid orbit activity window"));
        drop(db);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn imported_source_metadata_requires_sensitive_opt_in_without_changing_memory_count() {
        let db_path = std::env::temp_dir().join(format!(
            "agentic-os-orbit-source-visibility-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Db::open(&db_path).unwrap();
        crate::memory::index::ensure_tables(&db).unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO document_imports (
                    id, domain, title, input_kind, source_ref, source_path, content_hash,
                    byte_count, candidate_count, warning_count, warnings_json, status,
                    created_at, updated_at
                 ) VALUES (?1, 'work', 'Private source', 'email', 'mail:test',
                    'sources/work/private.md', 'hash', 42, 0, 0, '[]', 'completed', ?2, ?2)",
                params!["import-1", chrono::Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })
        .unwrap();

        let hidden = build(&db, Some("work"), false, None).unwrap();
        assert!(hidden.nodes.iter().all(|node| node.kind != "source"));
        let visible = build(&db, Some("work"), true, None).unwrap();
        let source = visible
            .nodes
            .iter()
            .find(|node| node.id == "source:import-1")
            .unwrap();
        assert_eq!(source.sensitivity.as_deref(), Some("sensitive"));
        assert!(source.preview.is_none());
        assert_eq!(visible.counts.memories, hidden.counts.memories);

        drop(db);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn domain_filter_is_applied_before_activity_counts_and_traces() {
        let db_path = std::env::temp_dir().join(format!(
            "agentic-os-orbit-domain-activity-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Db::open(&db_path).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            for (id, domain) in [("task-work", "work"), ("task-finance", "finance")] {
                conn.execute(
                    "INSERT INTO tasks (
                        id, title, goal, domain, harness, status, origin_kind, sandbox_mode,
                        cwd, risk_level, created_at, updated_at
                     ) VALUES (?1, ?2, 'test', ?3, 'codex', 'completed', 'manual',
                        'read-only', '/tmp', 'low', ?4, ?4)",
                    params![id, format!("{domain} task"), domain, now],
                )?;
            }
            Ok(())
        })
        .unwrap();
        crate::audit::append_row(
            &db,
            "run-work",
            "task-work",
            "context",
            "work context",
            &serde_json::json!({"memoryRefs":[]}),
            None,
            None,
        )
        .unwrap();
        crate::audit::append_row(
            &db,
            "run-finance",
            "task-finance",
            "context",
            "finance context",
            &serde_json::json!({"memoryRefs":[]}),
            None,
            None,
        )
        .unwrap();

        let map = build(&db, Some("work"), false, Some("today")).unwrap();
        assert_eq!(map.metrics.tasks_scanned, 1);
        assert_eq!(map.metrics.traces_scanned, 1);
        assert_eq!(map.metrics.activity_events, 1);
        assert_eq!(map.activities.len(), 1);
        assert_eq!(map.activities[0].task_id, "task-work");
        assert!(map.nodes.iter().all(|node| {
            node.domain
                .as_deref()
                .map_or(true, |domain| domain == "work")
        }));

        drop(db);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn map_composes_from_the_real_registry_without_preview_nodes() {
        let db_path = std::env::temp_dir().join(format!(
            "agentic-os-orbit-real-registry-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Db::open(&db_path).unwrap();
        let map = build(&db, None, false, None).unwrap();
        let top_level = map
            .nodes
            .iter()
            .filter(|node| node.group_id.is_none())
            .count();
        let skill_groups = map
            .nodes
            .iter()
            .filter(|node| node.kind == "skill_group")
            .count();
        let application_groups = map
            .nodes
            .iter()
            .filter(|node| node.kind == "application_group")
            .count();
        println!(
            "MILESTONE2_ORBIT={{\"skills\":{},\"memories\":{},\"routines\":{},\"applications\":{},\"relations\":{},\"topLevel\":{},\"skillGroups\":{},\"applicationGroups\":{},\"composeMs\":{:.3}}}",
            map.counts.skills,
            map.counts.memories,
            map.counts.routines,
            map.counts.applications,
            map.counts.relations,
            top_level,
            skill_groups,
            application_groups,
            map.metrics.compose_ms,
        );
        assert!(map.nodes.iter().any(|node| node.id == "core:agentic-os"));
        assert_eq!(map.activity_window, "today");
        assert_eq!(
            map.nodes
                .iter()
                .filter(|node| node.kind == "memory_domain")
                .count(),
            6
        );
        assert!(map.nodes.iter().all(|node| node.status != "preview"));
        drop(db);
        let _ = std::fs::remove_file(db_path);
    }
}
