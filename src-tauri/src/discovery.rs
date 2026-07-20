use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use serde_json::Value;
use walkdir::{DirEntry, WalkDir};

use crate::error::{AppError, AppResult};
use crate::models::{
    CatalogCounts, CatalogItem, CatalogKind, CatalogSection, RuntimeInfo, SourceDescriptor,
    SourceStatus,
};

const MAX_WORKSPACE_ROOTS: usize = 24;
const MAX_SCAN_DEPTH: usize = 5;
const MAX_MANIFEST_DEPTH: usize = 4;
const MAX_SUMMARY_BYTES: u64 = 512 * 1024;
const ALLOWLIST_CONFIG_RELATIVE: &str = ".agent-control/scan-roots.json";
const CANONICAL_DIRS: [&str; 6] = ["agents", "prompts", "routines", "skills", "tools", "workflows"];
const EXCLUDED_DISCOVERY_DIRS: [&str; 1] = ["Library/CloudStorage"];

#[derive(Debug, Clone, Deserialize, Default)]
struct MarketplaceIndex {
    #[serde(default)]
    plugins: Vec<MarketplacePlugin>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct MarketplacePlugin {
    name: String,
    category: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PluginManifest {
    name: String,
    version: Option<String>,
    description: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    interface: Option<PluginInterface>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PluginInterface {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "shortDescription")]
    short_description: Option<String>,
    category: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct AllowlistConfig {
    #[serde(default)]
    roots: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PackageManifest {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    #[serde(default)]
    scripts: HashMap<String, String>,
    #[serde(default)]
    dependencies: HashMap<String, String>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct PyprojectManifest {
    description: Option<String>,
    dependencies: Vec<String>,
    name: Option<String>,
    version: Option<String>,
    has_crewai: bool,
    text: String,
}

#[derive(Debug, Clone)]
struct McpServerSpec {
    entrypoint: Option<String>,
    provider: String,
    summary: Option<String>,
    tags: Vec<String>,
}

pub struct DiscoveryBundle {
    pub catalog: CatalogSection,
    pub runtime: RuntimeInfo,
    pub sources: Vec<SourceDescriptor>,
}

pub fn discover() -> AppResult<DiscoveryBundle> {
    let home_dir = dirs::home_dir().ok_or(AppError::MissingHomeDirectory)?;
    let codex_home = home_dir.join(".codex");
    let claude_home = home_dir.join(".claude");
    let gemini_home = home_dir.join(".gemini");
    let n8n_home = home_dir.join(".n8n");
    let allowlist_config = home_dir.join(ALLOWLIST_CONFIG_RELATIVE);
    let workspace_roots = discover_workspace_roots(
        &home_dir,
        &codex_home,
        &claude_home,
        &allowlist_config,
        env::current_dir().ok(),
    );

    let mut items = Vec::new();
    let mut sources = Vec::new();

    register_path_source(
        &mut sources,
        "scanner-config",
        "Scanner allowlist",
        "scanner",
        &allowlist_config,
        &home_dir,
    );
    register_text_source(
        &mut sources,
        "workspace-allowlist",
        "Allowlisted workspaces",
        "scanner",
        format!("{} roots", workspace_roots.len()),
        !workspace_roots.is_empty(),
    );
    register_path_source(
        &mut sources,
        "codex-home",
        "Codex home",
        "vendor",
        &codex_home,
        &home_dir,
    );
    register_path_source(
        &mut sources,
        "claude-home",
        "Claude home",
        "vendor",
        &claude_home,
        &home_dir,
    );
    register_path_source(
        &mut sources,
        "gemini-home",
        "Gemini home",
        "vendor",
        &gemini_home,
        &home_dir,
    );
    register_path_source(&mut sources, "n8n-home", "n8n home", "vendor", &n8n_home, &home_dir);

    items.extend(run_codex_detector(&home_dir, &codex_home, &workspace_roots));
    items.extend(run_claude_detector(&home_dir, &claude_home, &workspace_roots));
    items.extend(run_gemini_detector(&home_dir, &workspace_roots));
    items.extend(run_mcp_detector(
        &home_dir,
        &claude_home,
        &workspace_roots,
    ));
    items.extend(run_n8n_detector(&home_dir, &n8n_home, &workspace_roots));
    items.extend(run_local_agents_detector(&home_dir, &workspace_roots));
    items.extend(run_custom_rules_detector(&home_dir, &workspace_roots));

    let mut deduped: HashMap<String, CatalogItem> = HashMap::new();
    for item in items {
        if let Some(existing) = deduped.get_mut(&item.id) {
            *existing = merge_catalog_items(existing.clone(), item);
        } else {
            deduped.insert(item.id.clone(), item);
        }
    }

    let mut items = deduped.into_values().collect::<Vec<_>>();
    items.sort_by(|left, right| {
        kind_rank(left.kind)
            .cmp(&kind_rank(right.kind))
            .then_with(|| left.display_name.cmp(&right.display_name))
            .then_with(|| left.path.cmp(&right.path))
    });

    let counts = build_counts(&items);
    let total_items = items.len() as i64;

    Ok(DiscoveryBundle {
        catalog: CatalogSection {
            counts,
            items,
            total_items,
        },
        runtime: RuntimeInfo {
            platform: format!("{} {}", env::consts::OS, env::consts::ARCH),
            codex_home: display_path(&codex_home, &home_dir),
        },
        sources,
    })
}

fn run_codex_detector(home_dir: &Path, codex_home: &Path, workspace_roots: &[PathBuf]) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    if !codex_home.exists() {
        return items;
    }

    let marketplace_root = codex_home.join(".tmp/plugins/plugins");
    let marketplace_index_path = codex_home.join(".tmp/plugins/.agents/plugins/marketplace.json");
    let marketplace_index = load_marketplace_index(&marketplace_index_path);

    let codex_skills = codex_home.join("skills");
    items.extend(discover_skill_documents(
        &codex_skills,
        home_dir,
        "Codex skills",
        "Codex",
        "codex",
        "codex",
    ));

    items.extend(discover_plugin_directories(
        &marketplace_root,
        home_dir,
        "Marketplace cache",
        "Marketplace",
        marketplace_index.as_ref(),
        "codex",
        "codex",
    ));
    items.extend(discover_skill_documents(
        &marketplace_root,
        home_dir,
        "Marketplace cache",
        "Marketplace",
        "codex",
        "codex",
    ));

    let codex_routines = codex_home.join("routines");
    items.extend(discover_direct_items(
        &codex_routines,
        CatalogKind::Routine,
        home_dir,
        "Codex routines",
        "Codex",
        "codex",
        "codex",
    ));

    for workspace_root in workspace_roots {
        let group = workspace_display_name(workspace_root);
        let origin = format!("{group} plugins");
        items.extend(discover_codex_workspace_plugins(
            workspace_root,
            home_dir,
            &origin,
            &group,
        ));
    }

    items
}

fn run_claude_detector(home_dir: &Path, claude_home: &Path, workspace_roots: &[PathBuf]) -> Vec<CatalogItem> {
    let mut items = discover_named_documents(
        workspace_roots,
        "CLAUDE.md",
        CatalogKind::Prompt,
        home_dir,
        "Claude instructions",
        "claude",
        "claude",
        0.96,
    );

    let installed_plugins = claude_home.join("plugins/installed_plugins.json");
    items.extend(discover_claude_plugins(
        &installed_plugins,
        home_dir,
        "Claude plugins",
    ));

    items
}

fn run_gemini_detector(home_dir: &Path, workspace_roots: &[PathBuf]) -> Vec<CatalogItem> {
    discover_named_documents(
        workspace_roots,
        "GEMINI.md",
        CatalogKind::Prompt,
        home_dir,
        "Gemini instructions",
        "gemini",
        "gemini",
        0.96,
    )
}

fn run_mcp_detector(home_dir: &Path, claude_home: &Path, workspace_roots: &[PathBuf]) -> Vec<CatalogItem> {
    let mut items = Vec::new();

    for workspace_root in workspace_roots {
        items.extend(discover_workspace_mcp_configs(workspace_root, home_dir));
    }

    let claude_state = home_dir.join(".claude.json");
    items.extend(discover_claude_project_mcp_servers(
        &claude_state,
        workspace_roots,
        home_dir,
    ));

    items.extend(discover_mcp_manifest_candidates(
        workspace_roots,
        home_dir,
        "mcp",
        "mcp",
    ));

    let claude_config = claude_home.join("settings.json");
    if claude_config.exists() {
        items.extend(discover_mcp_servers_from_config(
            &claude_config,
            home_dir,
            "Claude settings",
            "Claude",
            "claude",
            "mcp",
        ));
    }

    items
}

fn run_n8n_detector(home_dir: &Path, n8n_home: &Path, workspace_roots: &[PathBuf]) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    let custom_nodes_manifest = n8n_home.join("nodes/package.json");
    items.extend(discover_n8n_nodes_package(
        &custom_nodes_manifest,
        home_dir,
        "n8n custom nodes",
    ));

    items.extend(discover_n8n_workflows(&[n8n_home.to_path_buf()], home_dir, "n8n", "n8n"));
    items.extend(discover_n8n_workflows(workspace_roots, home_dir, "workspace", "n8n"));

    items
}

fn run_local_agents_detector(home_dir: &Path, workspace_roots: &[PathBuf]) -> Vec<CatalogItem> {
    let mut items = Vec::new();

    items.extend(discover_named_documents(
        workspace_roots,
        "AGENTS.md",
        CatalogKind::Agent,
        home_dir,
        "Agent instructions",
        "local",
        "local-agents",
        0.88,
    ));

    for workspace_root in workspace_roots {
        let group = workspace_display_name(workspace_root);

        items.extend(discover_skill_documents(
            workspace_root,
            home_dir,
            &format!("{group} skills"),
            &group,
            "local",
            "local-agents",
        ));
        items.extend(discover_direct_children_in_named_dirs(
            workspace_root,
            "agents",
            CatalogKind::Agent,
            home_dir,
            &format!("{group} agents"),
            "local",
            "local-agents",
            0.82,
        ));
        items.extend(discover_direct_children_in_named_dirs(
            workspace_root,
            "prompts",
            CatalogKind::Prompt,
            home_dir,
            &format!("{group} prompts"),
            "local",
            "local-agents",
            0.84,
        ));
        items.extend(discover_direct_children_in_named_dirs(
            workspace_root,
            "routines",
            CatalogKind::Routine,
            home_dir,
            &format!("{group} routines"),
            "local",
            "local-agents",
            0.86,
        ));
        items.extend(discover_direct_children_in_named_dirs(
            workspace_root,
            "workflows",
            CatalogKind::Workflow,
            home_dir,
            &format!("{group} workflows"),
            "local",
            "local-agents",
            0.86,
        ));
        items.extend(discover_direct_children_in_named_dirs(
            workspace_root,
            "tools",
            CatalogKind::Automation,
            home_dir,
            &format!("{group} tools"),
            "local",
            "local-agents",
            0.76,
        ));
    }

    items
}

fn run_custom_rules_detector(home_dir: &Path, workspace_roots: &[PathBuf]) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    items.extend(discover_agentic_package_manifests(workspace_roots, home_dir));
    items.extend(discover_agentic_pyprojects(workspace_roots, home_dir));
    items.extend(discover_script_automations(workspace_roots, home_dir));
    items
}

fn discover_workspace_roots(
    home_dir: &Path,
    codex_home: &Path,
    claude_home: &Path,
    allowlist_config: &Path,
    current_dir: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(current_dir) = current_dir.filter(|path| is_workspace_candidate(path, home_dir)) {
        roots.push(current_dir);
    }

    roots.extend(load_allowlist_roots(allowlist_config, home_dir));
    roots.extend(discover_codex_workspace_roots(codex_home, home_dir));
    roots.extend(discover_claude_workspace_roots(claude_home, home_dir));

    dedupe_workspace_roots(roots)
}

fn load_allowlist_roots(path: &Path, home_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if !path.exists() {
        return roots;
    }

    if let Some(config) = try_read_json::<AllowlistConfig>(path) {
        roots.extend(
            config
                .roots
                .into_iter()
                .map(|root| expand_path(&root, home_dir))
                .filter(|root| is_workspace_candidate(root, home_dir)),
        );
    } else if let Some(raw_roots) = try_read_json::<Vec<String>>(path) {
        roots.extend(
            raw_roots
                .into_iter()
                .map(|root| expand_path(&root, home_dir))
                .filter(|root| is_workspace_candidate(root, home_dir)),
        );
    }

    roots
}

fn discover_codex_workspace_roots(codex_home: &Path, home_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let state_path = codex_home.join("state_5.sqlite");
    let Ok(Some(connection)) = open_readonly(&state_path) else {
        return roots;
    };
    let Ok(true) = table_exists(&connection, "threads") else {
        return roots;
    };

    let Ok(mut statement) = connection.prepare(
        "SELECT cwd
         FROM threads
         WHERE cwd != ''
         GROUP BY cwd
         ORDER BY MAX(COALESCE(updated_at_ms, updated_at * 1000)) DESC
         LIMIT ?1",
    ) else {
        return roots;
    };

    let Ok(rows) = statement.query_map([MAX_WORKSPACE_ROOTS as i64], |row| row.get::<_, String>(0))
    else {
        return roots;
    };

    for row in rows.flatten() {
        let path = PathBuf::from(row);
        if is_workspace_candidate(&path, home_dir) {
            roots.push(path);
        }
    }

    roots
}

fn discover_claude_workspace_roots(claude_home: &Path, home_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    let claude_state = home_dir.join(".claude.json");
    if let Some(value) = try_read_json_value(&claude_state) {
        if let Some(projects) = value.get("projects").and_then(|value| value.as_object()) {
            for project_path in projects.keys() {
                let path = PathBuf::from(project_path);
                if is_workspace_candidate(&path, home_dir) {
                    roots.push(path);
                }
            }
        }
    }

    let projects_dir = claude_home.join("projects");
    if projects_dir.exists() {
        for entry in WalkDir::new(&projects_dir)
            .max_depth(3)
            .into_iter()
            .filter_entry(should_visit)
            .flatten()
        {
            if !entry.file_type().is_file() || entry.file_name() != "sessions-index.json" {
                continue;
            }

            if let Some(value) = try_read_json_value(entry.path()) {
                if let Some(original_path) = value.get("originalPath").and_then(|value| value.as_str())
                {
                    let path = PathBuf::from(original_path);
                    if is_workspace_candidate(&path, home_dir) {
                        roots.push(path);
                    }
                }
            }
        }
    }

    roots
}

fn discover_skill_documents(
    root: &Path,
    home_dir: &Path,
    origin: &str,
    group: &str,
    provider: &str,
    detector: &str,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    if !root.exists() {
        return items;
    }

    for entry in WalkDir::new(root)
        .max_depth(MAX_SCAN_DEPTH)
        .into_iter()
        .filter_entry(should_visit)
        .flatten()
    {
        if !entry.file_type().is_file() || entry.file_name() != "SKILL.md" {
            continue;
        }

        let path = entry.path().to_path_buf();
        let display_name = path
            .parent()
            .and_then(|parent| parent.file_name())
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "skill".into());
        let summary = extract_skill_summary(&path);
        let provider_name = choose_provider(provider, infer_provider_from_text(summary.as_deref().unwrap_or("")));

        items.push(build_item(
            CatalogKind::Skill,
            &path,
            home_dir,
            display_name.clone(),
            display_name,
            summary,
            origin.to_string(),
            group.to_string(),
            infer_extension_tags(&path),
            None,
            Some("skill-pack".into()),
            provider_name.to_string(),
            detector.to_string(),
            Some(display_path(&path, home_dir)),
            0.98,
        ));
    }

    items
}

fn discover_plugin_directories(
    root: &Path,
    home_dir: &Path,
    origin: &str,
    group: &str,
    marketplace_index: Option<&HashMap<String, MarketplacePlugin>>,
    provider: &str,
    detector: &str,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    if !root.exists() {
        return items;
    }

    let Ok(entries) = fs::read_dir(root) else {
        return items;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        if let Some(item) = build_plugin_item(
            &entry.path(),
            home_dir,
            origin,
            group,
            marketplace_index,
            provider,
            detector,
        ) {
            items.push(item);
        }
    }

    items
}

fn discover_codex_workspace_plugins(
    workspace_root: &Path,
    home_dir: &Path,
    origin: &str,
    group: &str,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    let mut candidates = Vec::new();

    if is_codex_plugin_dir(workspace_root) {
        candidates.push(workspace_root.to_path_buf());
    }

    let Ok(entries) = fs::read_dir(workspace_root) else {
        return items;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        if is_codex_plugin_dir(&entry.path()) {
            candidates.push(entry.path());
        }
    }

    for candidate in candidates {
        if let Some(item) = build_plugin_item(
            &candidate,
            home_dir,
            origin,
            group,
            None,
            "codex",
            "codex",
        ) {
            items.push(item);
        }
    }

    items
}

fn discover_direct_items(
    root: &Path,
    kind: CatalogKind,
    home_dir: &Path,
    origin: &str,
    group: &str,
    provider: &str,
    detector: &str,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    if !root.exists() {
        return items;
    }

    let Ok(entries) = fs::read_dir(root) else {
        return items;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .map(|value| value.to_string_lossy().starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }

        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() && !is_supported_file_for_kind(&path, kind) {
            continue;
        }

        let display_name = path
            .file_stem()
            .or_else(|| path.file_name())
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| kind_label(kind).to_lowercase());
        let summary = if file_type.is_dir() {
            extract_dir_summary(&path)
        } else {
            extract_file_summary(&path)
        };

        items.push(build_item(
            kind,
            &path,
            home_dir,
            display_name.clone(),
            display_name,
            summary,
            origin.to_string(),
            group.to_string(),
            infer_tags_for_entry(&path, kind),
            None,
            Some(kind_label(kind).to_lowercase()),
            provider.to_string(),
            detector.to_string(),
            detect_entrypoint_for_path(&path, home_dir),
            kind_confidence(kind),
        ));
    }

    items
}

fn discover_named_documents(
    workspace_roots: &[PathBuf],
    file_name: &str,
    kind: CatalogKind,
    home_dir: &Path,
    origin_label: &str,
    provider: &str,
    detector: &str,
    confidence: f64,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();

    for workspace_root in workspace_roots {
        let group = workspace_display_name(workspace_root);
        for entry in WalkDir::new(workspace_root)
            .max_depth(MAX_SCAN_DEPTH)
            .into_iter()
            .filter_entry(should_visit)
            .flatten()
        {
            if !entry.file_type().is_file() || entry.file_name() != file_name {
                continue;
            }

            let path = entry.path().to_path_buf();
            let summary = extract_markdown_summary(&path);
            let display_name = workspace_display_name(
                path.parent().unwrap_or(workspace_root),
            );

            items.push(build_item(
                kind,
                &path,
                home_dir,
                display_name.clone(),
                display_name,
                summary.or_else(|| Some(format!("{origin_label} discovered in the local workspace."))),
                format!("{group} {}", kind_label(kind).to_lowercase()),
                group.clone(),
                infer_extension_tags(&path),
                None,
                Some(file_name.to_lowercase()),
                provider.to_string(),
                detector.to_string(),
                Some(display_path(&path, home_dir)),
                confidence,
            ));
        }
    }

    items
}

fn discover_workspace_mcp_configs(workspace_root: &Path, home_dir: &Path) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    let candidates = [
        workspace_root.join(".mcp.json"),
        workspace_root.join("mcp.json"),
        workspace_root.join(".vscode/mcp.json"),
    ];

    for candidate in candidates {
        items.extend(discover_mcp_servers_from_config(
            &candidate,
            home_dir,
            &format!("{} MCP config", workspace_display_name(workspace_root)),
            &workspace_display_name(workspace_root),
            "local",
            "mcp",
        ));
    }

    items
}

fn discover_claude_project_mcp_servers(
    claude_state_path: &Path,
    workspace_roots: &[PathBuf],
    home_dir: &Path,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    let Some(value) = try_read_json_value(claude_state_path) else {
        return items;
    };
    let Some(projects) = value.get("projects").and_then(|value| value.as_object()) else {
        return items;
    };

    let allowlist = workspace_roots
        .iter()
        .map(|path| fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
        .collect::<Vec<_>>();

    for (project_path, project_value) in projects {
        let project_root = PathBuf::from(project_path);
        let canonical = fs::canonicalize(&project_root).unwrap_or(project_root.clone());
        if !allowlist.iter().any(|root| root == &canonical) {
            continue;
        }

        let Some(servers) = project_value.get("mcpServers").and_then(|value| value.as_object()) else {
            continue;
        };

        let group = workspace_display_name(&project_root);
        for (server_name, server_value) in servers {
            let spec = extract_mcp_server(server_name, server_value, "claude");
            items.push(build_item(
                CatalogKind::Mcp,
                claude_state_path,
                home_dir,
                server_name.clone(),
                server_name.clone(),
                spec.summary.or_else(|| Some("MCP server declared in Claude project state.".into())),
                "Claude project state".into(),
                group.clone(),
                spec.tags,
                None,
                Some("mcp-server".into()),
                spec.provider,
                "claude".into(),
                spec.entrypoint,
                0.9,
            ));
        }
    }

    items
}

fn discover_mcp_servers_from_config(
    path: &Path,
    home_dir: &Path,
    origin: &str,
    group: &str,
    provider: &str,
    detector: &str,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    let Some(value) = try_read_json_value(path) else {
        return items;
    };

    let Some(servers) = value
        .get("servers")
        .or_else(|| value.get("mcpServers"))
        .and_then(|value| value.as_object())
    else {
        return items;
    };

    for (server_name, server_value) in servers {
        let spec = extract_mcp_server(server_name, server_value, provider);
        items.push(build_item(
            CatalogKind::Mcp,
            path,
            home_dir,
            server_name.clone(),
            server_name.clone(),
            spec.summary.or_else(|| Some("MCP server discovered in a local configuration file.".into())),
            origin.to_string(),
            group.to_string(),
            spec.tags,
            None,
            Some("mcp-server".into()),
            spec.provider,
            detector.to_string(),
            spec.entrypoint,
            0.98,
        ));
    }

    items
}

fn discover_n8n_nodes_package(path: &Path, home_dir: &Path, origin: &str) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    let Some(manifest) = try_read_json::<PackageManifest>(path) else {
        return items;
    };

    for dependency in manifest
        .dependencies
        .keys()
        .filter(|dependency| dependency.starts_with("n8n-nodes-"))
    {
        items.push(build_item(
            CatalogKind::Plugin,
            path,
            home_dir,
            dependency.clone(),
            dependency.clone(),
            Some("Custom n8n node package installed locally.".into()),
            origin.to_string(),
            "n8n".into(),
            vec!["n8n".into(), "nodes".into()],
            None,
            Some("n8n-node".into()),
            "n8n".into(),
            "n8n".into(),
            Some(display_path(path, home_dir)),
            0.94,
        ));
    }

    items
}

fn discover_n8n_workflows(
    workspace_roots: &[PathBuf],
    home_dir: &Path,
    provider: &str,
    detector: &str,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();

    for workspace_root in workspace_roots {
        for entry in WalkDir::new(workspace_root)
            .max_depth(MAX_SCAN_DEPTH)
            .into_iter()
            .filter_entry(should_visit)
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path().to_path_buf();
            let file_name = path
                .file_name()
                .map(|value| value.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let in_workflows_dir = relative_has_component(
                path.strip_prefix(workspace_root).unwrap_or(&path),
                "workflows",
            );

            if !(file_name.ends_with(".n8n.json") || in_workflows_dir) {
                continue;
            }

            let Some(value) = try_read_json_value(&path) else {
                continue;
            };
            let has_nodes = value.get("nodes").and_then(|value| value.as_array()).is_some();
            let has_connections = value.get("connections").is_some();
            if !has_nodes || !has_connections {
                continue;
            }

            let display_name = value
                .get("name")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
                .unwrap_or_else(|| {
                    path.file_stem()
                        .map(|value| value.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "workflow".into())
                });

            let summary = value
                .get("meta")
                .and_then(|value| value.get("description"))
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
                .or_else(|| Some("n8n workflow discovered from a local JSON workflow export.".into()));

            items.push(build_item(
                CatalogKind::Workflow,
                &path,
                home_dir,
                display_name.clone(),
                display_name,
                summary,
                format!("{} workflows", workspace_display_name(workspace_root)),
                workspace_display_name(workspace_root),
                vec!["n8n".into(), "workflow".into(), "json".into()],
                None,
                Some("n8n-workflow".into()),
                provider.to_string(),
                detector.to_string(),
                Some(display_path(&path, home_dir)),
                0.97,
            ));
        }
    }

    items
}

fn discover_direct_children_in_named_dirs(
    workspace_root: &Path,
    dir_name: &str,
    kind: CatalogKind,
    home_dir: &Path,
    origin: &str,
    provider: &str,
    detector: &str,
    confidence: f64,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();

    for base_dir in matching_named_dirs(workspace_root, dir_name) {
        let Ok(entries) = fs::read_dir(&base_dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name.starts_with('.') {
                continue;
            }

            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_file() && !is_supported_file_for_kind(&path, kind) {
                continue;
            }

            let summary = if file_type.is_dir() {
                extract_dir_summary(&path)
            } else {
                extract_file_summary(&path)
            };
            let display_name = path
                .file_stem()
                .or_else(|| path.file_name())
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| kind_label(kind).to_lowercase());

            items.push(build_item(
                kind,
                &path,
                home_dir,
                display_name.clone(),
                display_name,
                summary.or_else(|| {
                    Some(format!(
                        "{} discovered in the local {} directory.",
                        kind_label(kind),
                        dir_name
                    ))
                }),
                origin.to_string(),
                workspace_display_name(workspace_root),
                infer_tags_for_entry(&path, kind),
                None,
                Some(dir_name.to_string()),
                choose_provider(provider, infer_provider_from_path(&path)).to_string(),
                detector.to_string(),
                detect_entrypoint_for_path(&path, home_dir),
                confidence,
            ));
        }
    }

    items
}

fn discover_agentic_package_manifests(workspace_roots: &[PathBuf], home_dir: &Path) -> Vec<CatalogItem> {
    let mut items = Vec::new();

    for manifest_path in walk_manifest_files(workspace_roots, "package.json") {
        if should_skip_manifest_candidate(&manifest_path) {
            continue;
        }

        let Some(manifest) = try_read_json::<PackageManifest>(&manifest_path) else {
            continue;
        };
        let Some(candidate) = classify_package_manifest(&manifest, &manifest_path, home_dir, false)
        else {
            continue;
        };
        items.push(candidate);
    }

    items
}

fn discover_agentic_pyprojects(workspace_roots: &[PathBuf], home_dir: &Path) -> Vec<CatalogItem> {
    let mut items = Vec::new();

    for manifest_path in walk_manifest_files(workspace_roots, "pyproject.toml") {
        if should_skip_manifest_candidate(&manifest_path) {
            continue;
        }

        let Some(manifest) = parse_pyproject_manifest(&manifest_path) else {
            continue;
        };
        let Some(candidate) = classify_pyproject_manifest(&manifest, &manifest_path, home_dir, false)
        else {
            continue;
        };
        items.push(candidate);
    }

    items
}

fn discover_mcp_manifest_candidates(
    workspace_roots: &[PathBuf],
    home_dir: &Path,
    provider: &str,
    detector: &str,
) -> Vec<CatalogItem> {
    let mut items = Vec::new();

    for manifest_path in walk_manifest_files(workspace_roots, "package.json") {
        let Some(manifest) = try_read_json::<PackageManifest>(&manifest_path) else {
            continue;
        };
        let Some(candidate) = classify_package_manifest(&manifest, &manifest_path, home_dir, true)
        else {
            continue;
        };
        items.push(with_detector(candidate, provider, detector));
    }

    for manifest_path in walk_manifest_files(workspace_roots, "pyproject.toml") {
        let Some(manifest) = parse_pyproject_manifest(&manifest_path) else {
            continue;
        };
        let Some(candidate) = classify_pyproject_manifest(&manifest, &manifest_path, home_dir, true)
        else {
            continue;
        };
        items.push(with_detector(candidate, provider, detector));
    }

    items
}

fn discover_script_automations(workspace_roots: &[PathBuf], home_dir: &Path) -> Vec<CatalogItem> {
    let mut items = Vec::new();

    for workspace_root in workspace_roots {
        let candidates = [
            workspace_root.join("scripts"),
            workspace_root.to_path_buf(),
        ];

        for candidate_root in candidates {
            if !candidate_root.exists() {
                continue;
            }

            let Ok(entries) = fs::read_dir(&candidate_root) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if !file_type.is_file() || !is_script_candidate(&path) {
                    continue;
                }

                let summary = analyze_script_candidate(&path);
                let display_name = path
                    .file_stem()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "automation".into());

                let confidence = if summary.is_some() { 0.7 } else { 0.58 };
                items.push(build_item(
                    CatalogKind::Automation,
                    &path,
                    home_dir,
                    display_name.clone(),
                    display_name,
                    summary.or_else(|| Some("Script candidate for a local automation or agent run.".into())),
                    format!("{} scripts", workspace_display_name(workspace_root)),
                    workspace_display_name(workspace_root),
                    infer_extension_tags(&path),
                    None,
                    Some("script".into()),
                    infer_provider_from_path(&path).to_string(),
                    "custom-rules".into(),
                    Some(display_path(&path, home_dir)),
                    confidence,
                ));
            }
        }
    }

    items
}

fn discover_claude_plugins(path: &Path, home_dir: &Path, origin: &str) -> Vec<CatalogItem> {
    let mut items = Vec::new();
    let Some(value) = try_read_json_value(path) else {
        return items;
    };

    let Some(plugin_values) = value
        .get("plugins")
        .or_else(|| value.get("repositories"))
        .and_then(|value| value.as_object())
    else {
        return items;
    };

    for (plugin_name, plugin_value) in plugin_values {
        let display_name = plugin_value
            .get("displayName")
            .or_else(|| plugin_value.get("name"))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .unwrap_or_else(|| plugin_name.clone());
        let summary = plugin_value
            .get("description")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .or_else(|| Some("Claude plugin registered locally.".into()));
        let version = plugin_value
            .get("version")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());

        items.push(build_item(
            CatalogKind::Plugin,
            path,
            home_dir,
            plugin_name.clone(),
            display_name,
            summary,
            origin.into(),
            "Claude".into(),
            vec!["claude".into(), "plugin".into()],
            version,
            Some("claude-plugin".into()),
            "claude".into(),
            "claude".into(),
            Some(display_path(path, home_dir)),
            0.9,
        ));
    }

    items
}

fn classify_package_manifest(
    manifest: &PackageManifest,
    path: &Path,
    home_dir: &Path,
    mcp_only: bool,
) -> Option<CatalogItem> {
    let dependencies = manifest
        .dependencies
        .keys()
        .chain(manifest.dev_dependencies.keys())
        .cloned()
        .collect::<Vec<_>>();
    let scripts = manifest
        .scripts
        .iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .collect::<Vec<_>>();
    let keywords = dependencies
        .iter()
        .chain(scripts.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    let name = manifest
        .name
        .clone()
        .or_else(|| path.parent().and_then(|parent| parent.file_name()).map(|value| value.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "package".into());

    if mcp_only {
        if !contains_mcp_signal(&keywords) {
            return None;
        }

        return Some(build_item(
            CatalogKind::Mcp,
            path,
            home_dir,
            name.clone(),
            name,
            manifest.description.clone().or_else(|| Some("Package manifest suggests MCP server or tooling support.".into())),
            format!("{} manifest", workspace_display_name(path.parent().unwrap_or(path))),
            workspace_display_name(path.parent().unwrap_or(path)),
            collect_manifest_tags(&dependencies, &scripts),
            manifest.version.clone(),
            Some("package-manifest".into()),
            infer_provider_from_text(&keywords).to_string(),
            "mcp".into(),
            first_relevant_script(&manifest.scripts),
            0.78,
        ));
    }

    let kind = if contains_n8n_signal(&keywords) {
        CatalogKind::Workflow
    } else if contains_prompt_signal(&keywords) {
        CatalogKind::Prompt
    } else if contains_workflow_signal(&keywords) {
        CatalogKind::Workflow
    } else if contains_agent_signal(&keywords) {
        CatalogKind::Agent
    } else if contains_automation_signal(&keywords) {
        CatalogKind::Automation
    } else {
        return None;
    };

    Some(build_item(
        kind,
        path,
        home_dir,
        name.clone(),
        name,
        manifest.description.clone().or_else(|| Some(format!(
            "{} manifest suggests a local {} project.",
            kind_label(kind),
            kind_label(kind).to_lowercase()
        ))),
        format!("{} manifest", workspace_display_name(path.parent().unwrap_or(path))),
        workspace_display_name(path.parent().unwrap_or(path)),
        collect_manifest_tags(&dependencies, &scripts),
        manifest.version.clone(),
        Some("package-manifest".into()),
        infer_provider_from_text(&keywords).to_string(),
        "custom-rules".into(),
        first_relevant_script(&manifest.scripts),
        if kind == CatalogKind::Agent { 0.82 } else { 0.72 },
    ))
}

fn classify_pyproject_manifest(
    manifest: &PyprojectManifest,
    path: &Path,
    home_dir: &Path,
    mcp_only: bool,
) -> Option<CatalogItem> {
    let keywords = manifest.text.to_ascii_lowercase();
    let name = manifest
        .name
        .clone()
        .or_else(|| path.parent().and_then(|parent| parent.file_name()).map(|value| value.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "pyproject".into());

    if mcp_only {
        if !contains_mcp_signal(&keywords) {
            return None;
        }

        return Some(build_item(
            CatalogKind::Mcp,
            path,
            home_dir,
            name.clone(),
            name,
            manifest
                .description
                .clone()
                .or_else(|| Some("Python project suggests MCP server or tooling support.".into())),
            format!("{} pyproject", workspace_display_name(path.parent().unwrap_or(path))),
            workspace_display_name(path.parent().unwrap_or(path)),
            manifest.dependencies.clone(),
            manifest.version.clone(),
            Some("pyproject".into()),
            infer_provider_from_text(&keywords).to_string(),
            "mcp".into(),
            Some(display_path(path, home_dir)),
            0.76,
        ));
    }

    let kind = if manifest.has_crewai {
        CatalogKind::Agent
    } else if contains_workflow_signal(&keywords) {
        CatalogKind::Workflow
    } else if contains_agent_signal(&keywords) {
        CatalogKind::Agent
    } else if contains_automation_signal(&keywords) {
        CatalogKind::Automation
    } else {
        return None;
    };

    Some(build_item(
        kind,
        path,
        home_dir,
        name.clone(),
        name,
        manifest.description.clone().or_else(|| Some(format!(
            "{} discovered from a Python project manifest.",
            kind_label(kind)
        ))),
        format!("{} pyproject", workspace_display_name(path.parent().unwrap_or(path))),
        workspace_display_name(path.parent().unwrap_or(path)),
        manifest.dependencies.clone(),
        manifest.version.clone(),
        Some("pyproject".into()),
        infer_provider_from_text(&keywords).to_string(),
        "custom-rules".into(),
        Some(display_path(path, home_dir)),
        if kind == CatalogKind::Agent { 0.8 } else { 0.68 },
    ))
}

fn parse_pyproject_manifest(path: &Path) -> Option<PyprojectManifest> {
    let text = try_read_to_string(path)?;
    let dependencies = extract_toml_array_values(&text, "dependencies");
    let description = extract_toml_string(&text, "description");
    let name = extract_toml_string(&text, "name");
    let version = extract_toml_string(&text, "version");

    Some(PyprojectManifest {
        description,
        dependencies,
        name,
        version,
        has_crewai: text.contains("[tool.crewai]"),
        text,
    })
}

fn build_plugin_item(
    plugin_dir: &Path,
    home_dir: &Path,
    origin: &str,
    group: &str,
    marketplace_index: Option<&HashMap<String, MarketplacePlugin>>,
    provider: &str,
    detector: &str,
) -> Option<CatalogItem> {
    let plugin_name = plugin_dir
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "plugin".into());
    if plugin_name.starts_with('.') {
        return None;
    }

    let manifest_path = plugin_dir.join(".codex-plugin/plugin.json");
    let app_manifest_path = plugin_dir.join(".app.json");
    let readme_path = plugin_dir.join("README.md");
    let manifest = try_read_json::<PluginManifest>(&manifest_path)
        .or_else(|| try_read_json::<PluginManifest>(&app_manifest_path));

    if manifest.is_none() && !readme_path.exists() {
        return None;
    }

    let summary = manifest
        .as_ref()
        .and_then(|value| {
            value
                .interface
                .as_ref()
                .and_then(|interface| interface.short_description.clone())
                .or_else(|| value.description.clone())
        })
        .or_else(|| extract_markdown_summary(&readme_path))
        .or_else(|| Some("Plugin discovered from a local plugin manifest.".into()));

    let display_name = manifest
        .as_ref()
        .and_then(|value| value.interface.as_ref().and_then(|interface| interface.display_name.clone()))
        .unwrap_or_else(|| plugin_name.clone());
    let category = manifest
        .as_ref()
        .and_then(|value| value.interface.as_ref().and_then(|interface| interface.category.clone()))
        .or_else(|| {
            marketplace_index
                .and_then(|index| index.get(&plugin_name))
                .and_then(|value| value.category.clone())
        });
    let version = manifest.as_ref().and_then(|value| value.version.clone());
    let tags = manifest
        .as_ref()
        .map(|value| value.keywords.clone())
        .unwrap_or_else(|| vec!["plugin".into()]);

    Some(build_item(
        CatalogKind::Plugin,
        plugin_dir,
        home_dir,
        manifest
            .as_ref()
            .map(|value| value.name.clone())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| plugin_name.clone()),
        display_name,
        summary,
        origin.to_string(),
        group.to_string(),
        tags,
        version,
        category,
        provider.to_string(),
        detector.to_string(),
        Some(display_path(plugin_dir, home_dir)),
        0.98,
    ))
}

fn build_item(
    kind: CatalogKind,
    path: &Path,
    home_dir: &Path,
    name: String,
    display_name: String,
    summary: Option<String>,
    origin: String,
    group: String,
    tags: Vec<String>,
    version: Option<String>,
    category: Option<String>,
    provider: String,
    detector: String,
    entrypoint: Option<String>,
    confidence: f64,
) -> CatalogItem {
    let id = build_id(path, home_dir, &name);

    CatalogItem {
        id,
        kind,
        name,
        display_name,
        summary: summary.map(normalize_summary),
        path: display_path(path, home_dir),
        origin,
        group,
        tags: dedupe_tags(tags),
        version,
        category,
        updated_at: modified_at(path),
        provider,
        detector,
        entrypoint,
        confidence,
    }
}

fn with_detector(mut item: CatalogItem, provider: &str, detector: &str) -> CatalogItem {
    item.provider = choose_provider(provider, &item.provider).to_string();
    item.detector = detector.to_string();
    item
}

fn merge_catalog_items(primary: CatalogItem, secondary: CatalogItem) -> CatalogItem {
    let (mut winner, other) = if should_prefer_item(&secondary, &primary) {
        (secondary, primary)
    } else {
        (primary, secondary)
    };

    winner.summary = choose_richer_option(winner.summary, other.summary);
    winner.version = choose_richer_option(winner.version, other.version);
    winner.category = choose_richer_option(winner.category, other.category);
    winner.entrypoint = choose_richer_option(winner.entrypoint, other.entrypoint);
    winner.origin = choose_richer_string(winner.origin, other.origin);
    winner.group = choose_richer_string(winner.group, other.group);
    winner.provider = choose_provider(&winner.provider, &other.provider).to_string();
    winner.tags.extend(other.tags);
    winner.tags = dedupe_tags(winner.tags);
    winner.updated_at = match (winner.updated_at, other.updated_at) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    };
    winner.confidence = winner.confidence.max(other.confidence);

    winner
}

fn should_prefer_item(candidate: &CatalogItem, current: &CatalogItem) -> bool {
    if candidate.confidence > current.confidence {
        return true;
    }

    if (candidate.confidence - current.confidence).abs() > f64::EPSILON {
        return false;
    }

    kind_rank(candidate.kind) < kind_rank(current.kind)
}

fn choose_richer_option(current: Option<String>, candidate: Option<String>) -> Option<String> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(choose_richer_string(current, candidate)),
        (Some(current), None) => Some(current),
        (None, Some(candidate)) => Some(candidate),
        (None, None) => None,
    }
}

fn choose_richer_string(current: String, candidate: String) -> String {
    if candidate.len() > current.len() {
        candidate
    } else {
        current
    }
}

fn build_counts(items: &[CatalogItem]) -> CatalogCounts {
    CatalogCounts {
        agent: count_by_kind(items, CatalogKind::Agent),
        automation: count_by_kind(items, CatalogKind::Automation),
        mcp: count_by_kind(items, CatalogKind::Mcp),
        plugin: count_by_kind(items, CatalogKind::Plugin),
        prompt: count_by_kind(items, CatalogKind::Prompt),
        routine: count_by_kind(items, CatalogKind::Routine),
        skill: count_by_kind(items, CatalogKind::Skill),
        workflow: count_by_kind(items, CatalogKind::Workflow),
    }
}

fn count_by_kind(items: &[CatalogItem], kind: CatalogKind) -> i64 {
    items.iter().filter(|item| item.kind == kind).count() as i64
}

fn register_path_source(
    sources: &mut Vec<SourceDescriptor>,
    id: &str,
    label: &str,
    kind: &str,
    path: &Path,
    home_dir: &Path,
) {
    sources.push(SourceDescriptor {
        id: id.into(),
        label: label.into(),
        kind: kind.into(),
        path: display_path(path, home_dir),
        status: if path.exists() {
            SourceStatus::Available
        } else {
            SourceStatus::Missing
        },
    });
}

fn register_text_source(
    sources: &mut Vec<SourceDescriptor>,
    id: &str,
    label: &str,
    kind: &str,
    text: String,
    available: bool,
) {
    sources.push(SourceDescriptor {
        id: id.into(),
        label: label.into(),
        kind: kind.into(),
        path: text,
        status: if available {
            SourceStatus::Available
        } else {
            SourceStatus::Missing
        },
    });
}

fn dedupe_workspace_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for root in roots {
        let canonical = fs::canonicalize(&root).unwrap_or(root.clone());
        let key = canonical.to_string_lossy().into_owned();
        if seen.insert(key) {
            deduped.push(canonical);
        }
        if deduped.len() >= MAX_WORKSPACE_ROOTS {
            break;
        }
    }

    deduped
}

fn is_workspace_candidate(path: &Path, home_dir: &Path) -> bool {
    if !path.is_absolute() || !path.is_dir() || path == Path::new("/") || path == home_dir {
        return false;
    }
    if is_excluded_discovery_path(path, home_dir) {
        return false;
    }
    if path.starts_with(home_dir.join(".codex"))
        || path.starts_with(home_dir.join(".claude"))
        || path.starts_with(home_dir.join(".gemini"))
        || path.starts_with(home_dir.join(".n8n"))
    {
        return false;
    }
    if path
        .components()
        .any(|component| component.as_os_str().to_string_lossy().ends_with(".app"))
    {
        return false;
    }

    let home_depth = component_depth(home_dir);
    let path_depth = component_depth(path);
    path_depth.saturating_sub(home_depth) >= 2
}

fn walk_manifest_files(workspace_roots: &[PathBuf], file_name: &str) -> Vec<PathBuf> {
    let mut manifests = Vec::new();

    for workspace_root in workspace_roots {
        for entry in WalkDir::new(workspace_root)
            .max_depth(MAX_MANIFEST_DEPTH)
            .into_iter()
            .filter_entry(should_visit)
            .flatten()
        {
            if !entry.file_type().is_file() || entry.file_name() != file_name {
                continue;
            }
            manifests.push(entry.path().to_path_buf());
        }
    }

    manifests
}

fn matching_named_dirs(workspace_root: &Path, dir_name: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if workspace_root
        .file_name()
        .map(|value| value == dir_name)
        .unwrap_or(false)
    {
        dirs.push(workspace_root.to_path_buf());
    }

    let nested = workspace_root.join(dir_name);
    if nested.exists() && nested.is_dir() {
        dirs.push(nested);
    }

    dirs
}

fn detect_entrypoint_for_path(path: &Path, home_dir: &Path) -> Option<String> {
    if path.is_file() {
        return Some(display_path(path, home_dir));
    }

    let preferred = [
        "agent.py",
        "agent.ts",
        "agent.tsx",
        "main.py",
        "main.ts",
        "main.tsx",
        "index.js",
        "index.ts",
        "index.tsx",
        "workflow.json",
        "README.md",
    ];

    for name in preferred {
        let candidate = path.join(name);
        if candidate.exists() {
            return Some(display_path(&candidate, home_dir));
        }
    }

    None
}

fn extract_dir_summary(path: &Path) -> Option<String> {
    for name in ["README.md", "AGENTS.md", "CLAUDE.md", "GEMINI.md", "SKILL.md"] {
        let candidate = path.join(name);
        if candidate.exists() {
            let summary = if name == "SKILL.md" {
                extract_skill_summary(&candidate)
            } else {
                extract_markdown_summary(&candidate)
            };
            if summary.is_some() {
                return summary;
            }
        }
    }

    let package_manifest = path.join("package.json");
    if let Some(manifest) = try_read_json::<PackageManifest>(&package_manifest) {
        if manifest.description.is_some() {
            return manifest.description;
        }
    }

    None
}

fn extract_file_summary(path: &Path) -> Option<String> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("md") => extract_markdown_summary(path),
        Some("json") => extract_json_description(path).or_else(|| extract_markdown_summary(path)),
        Some("toml") => parse_pyproject_manifest(path).and_then(|manifest| manifest.description),
        _ => extract_script_summary(path),
    }
}

fn extract_skill_summary(path: &Path) -> Option<String> {
    let content = try_read_to_string(path)?;
    if content.starts_with("---") {
        let mut lines = content.lines();
        let _ = lines.next();
        for line in lines {
            let trimmed = line.trim();
            if trimmed == "---" {
                break;
            }
            if let Some(description) = trimmed.strip_prefix("description:") {
                return Some(description.trim().trim_matches('"').to_string());
            }
        }
    }

    extract_markdown_summary(path)
}

fn extract_markdown_summary(path: &Path) -> Option<String> {
    let content = try_read_to_string(path)?;
    let mut in_frontmatter = false;
    let mut frontmatter_seen = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" && !frontmatter_seen {
            in_frontmatter = true;
            frontmatter_seen = true;
            continue;
        }
        if trimmed == "---" && in_frontmatter {
            in_frontmatter = false;
            continue;
        }
        if in_frontmatter || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return Some(trimmed.to_string());
    }

    None
}

fn extract_json_description(path: &Path) -> Option<String> {
    let value = try_read_json_value(path)?;
    value
        .get("description")
        .or_else(|| value.get("summary"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn extract_script_summary(path: &Path) -> Option<String> {
    let content = try_read_to_string(path)?;
    for line in content.lines().take(16) {
        let trimmed = line.trim().trim_start_matches('/').trim_start_matches('#').trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > 12 && !trimmed.starts_with("import ") && !trimmed.starts_with("from ") {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn analyze_script_candidate(path: &Path) -> Option<String> {
    let content = try_read_to_string(path)?;
    let lowered = content.to_ascii_lowercase();

    if lowered.contains("playwright") {
        return Some("Playwright-based local automation script.".into());
    }
    if lowered.contains("tool_calls")
        || lowered.contains("tools=[")
        || lowered.contains("tools = [")
        || lowered.contains("create_react_agent")
        || lowered.contains("openai-agents")
        || lowered.contains("autogen")
        || lowered.contains("langgraph")
    {
        return Some("Script shows agentic or tool-calling behavior.".into());
    }

    None
}

fn extract_toml_string(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix(&format!("{key} =")) {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

fn extract_toml_array_values(content: &str, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut in_array = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&format!("{key} = [")) {
            in_array = true;
        }
        if !in_array {
            continue;
        }

        if trimmed.ends_with(']') && trimmed != format!("{key} = [") {
            if let Some(value) = trimmed.split('"').nth(1) {
                values.push(value.to_string());
            }
            break;
        }

        if let Some(value) = trimmed.split('"').nth(1) {
            values.push(value.to_string());
        }
    }

    values
}

fn extract_mcp_server(_server_name: &str, value: &Value, default_provider: &str) -> McpServerSpec {
    let url = value
        .get("url")
        .and_then(|value| value.as_str())
        .map(sanitize_entrypoint);
    let command = value
        .get("command")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let entrypoint = url.or(command);
    let type_tag = value
        .get("type")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let provider = entrypoint
        .as_deref()
        .map(infer_provider_from_text)
        .unwrap_or(default_provider)
        .to_string();
    let summary = entrypoint
        .as_ref()
        .map(|entrypoint| format!("MCP server endpoint: {entrypoint}"));

    let mut tags = vec!["mcp".into()];
    if let Some(type_tag) = type_tag {
        tags.push(type_tag);
    }

    McpServerSpec {
        entrypoint,
        provider,
        summary,
        tags,
    }
}

fn collect_manifest_tags(dependencies: &[String], scripts: &[String]) -> Vec<String> {
    let mut tags = Vec::new();
    let interesting = [
        "agent",
        "autogen",
        "claude",
        "crewai",
        "gemini",
        "langgraph",
        "mcp",
        "n8n",
        "openai",
        "playwright",
        "prompt",
        "workflow",
    ];

    for keyword in interesting {
        if dependencies.iter().any(|value| value.to_ascii_lowercase().contains(keyword))
            || scripts.iter().any(|value| value.to_ascii_lowercase().contains(keyword))
        {
            tags.push(keyword.to_string());
        }
    }

    if tags.is_empty() {
        tags.push("manifest".into());
    }

    tags
}

fn first_relevant_script(scripts: &HashMap<String, String>) -> Option<String> {
    let priority = ["agent", "workflow", "prompt", "mcp", "n8n", "start", "dev", "run"];

    for keyword in priority {
        if let Some((name, value)) = scripts
            .iter()
            .find(|(name, value)| {
                name.to_ascii_lowercase().contains(keyword)
                    || value.to_ascii_lowercase().contains(keyword)
            })
        {
            return Some(format!("{name}: {value}"));
        }
    }

    None
}

fn infer_extension_tags(path: &Path) -> Vec<String> {
    path.extension()
        .map(|extension| vec![extension.to_string_lossy().into_owned()])
        .unwrap_or_default()
}

fn infer_tags_for_entry(path: &Path, kind: CatalogKind) -> Vec<String> {
    let mut tags = infer_extension_tags(path);
    if path.is_dir() {
        tags.push("dir".into());
    }
    if tags.is_empty() {
        tags.push(kind_label(kind).to_lowercase());
    }
    tags
}

fn infer_provider_from_text(text: &str) -> &'static str {
    let lowered = text.to_ascii_lowercase();
    if lowered.contains("claude") || lowered.contains("anthropic") {
        "claude"
    } else if lowered.contains("gemini") || lowered.contains("@google/genai") || lowered.contains("google-generativeai") {
        "gemini"
    } else if lowered.contains("openai") || lowered.contains("gpt") {
        "openai"
    } else if lowered.contains("crewai") {
        "crewai"
    } else if lowered.contains("langgraph") {
        "langgraph"
    } else if lowered.contains("autogen") {
        "autogen"
    } else if lowered.contains("n8n") {
        "n8n"
    } else if lowered.contains("mcp") || lowered.contains("modelcontextprotocol") {
        "mcp"
    } else if lowered.contains("playwright") {
        "playwright"
    } else {
        "local"
    }
}

fn infer_provider_from_path(path: &Path) -> &'static str {
    infer_provider_from_text(&path.to_string_lossy())
}

fn choose_provider<'a>(preferred: &'a str, fallback: &'a str) -> &'a str {
    if preferred == "local" && fallback != "local" {
        fallback
    } else {
        preferred
    }
}

fn contains_agent_signal(text: &str) -> bool {
    ["agent", "autogen", "crewai", "langgraph", "openai-agents", "semantic-kernel"]
        .iter()
        .any(|keyword| text.contains(keyword))
}

fn contains_workflow_signal(text: &str) -> bool {
    ["workflow", "langgraph", "dag", "pipeline"].iter().any(|keyword| text.contains(keyword))
}

fn contains_prompt_signal(text: &str) -> bool {
    ["prompt", "template"].iter().any(|keyword| text.contains(keyword))
}

fn contains_automation_signal(text: &str) -> bool {
    ["automation", "playwright", "script", "scrape", "runner"]
        .iter()
        .any(|keyword| text.contains(keyword))
}

fn contains_n8n_signal(text: &str) -> bool {
    ["n8n", "n8n-nodes-"].iter().any(|keyword| text.contains(keyword))
}

fn contains_mcp_signal(text: &str) -> bool {
    ["mcp", "modelcontextprotocol", "@modelcontextprotocol/sdk"]
        .iter()
        .any(|keyword| text.contains(keyword))
}

fn is_codex_plugin_dir(path: &Path) -> bool {
    path.join(".codex-plugin/plugin.json").exists() || path.join(".app.json").exists()
}

fn is_supported_file_for_kind(path: &Path, kind: CatalogKind) -> bool {
    match kind {
        CatalogKind::Agent => matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("json" | "md" | "py" | "toml" | "ts" | "tsx" | "yaml" | "yml")
        ),
        CatalogKind::Automation => matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("js" | "mjs" | "py" | "sh" | "ts" | "tsx")
        ),
        CatalogKind::Mcp => matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("json" | "md" | "py" | "toml" | "ts")
        ),
        CatalogKind::Plugin => matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("json" | "md" | "toml")
        ),
        CatalogKind::Prompt => matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("json" | "md" | "txt" | "yaml" | "yml")
        ),
        CatalogKind::Routine | CatalogKind::Workflow => matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("js" | "json" | "md" | "mjs" | "py" | "sh" | "ts" | "txt" | "yaml" | "yml")
        ),
        CatalogKind::Skill => path.file_name().map(|value| value == "SKILL.md").unwrap_or(false),
    }
}

fn is_script_candidate(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("js" | "mjs" | "py" | "sh" | "ts" | "tsx")
    ) {
        return false;
    }

    ["agent", "automation", "playwright", "prompt", "routine", "workflow"]
        .iter()
        .any(|keyword| file_name.contains(keyword))
}

fn should_skip_manifest_candidate(path: &Path) -> bool {
    CANONICAL_DIRS
        .iter()
        .any(|dir_name| path_has_component(path, dir_name))
}

fn path_has_component(path: &Path, value: &str) -> bool {
    path.components().any(|component| component.as_os_str() == value)
}

fn relative_has_component(path: &Path, value: &str) -> bool {
    path.components().any(|component| component.as_os_str() == value)
}

fn kind_rank(kind: CatalogKind) -> i32 {
    match kind {
        CatalogKind::Skill => 0,
        CatalogKind::Plugin => 1,
        CatalogKind::Agent => 2,
        CatalogKind::Routine => 3,
        CatalogKind::Workflow => 4,
        CatalogKind::Prompt => 5,
        CatalogKind::Mcp => 6,
        CatalogKind::Automation => 7,
    }
}

fn kind_confidence(kind: CatalogKind) -> f64 {
    match kind {
        CatalogKind::Skill => 0.9,
        CatalogKind::Plugin => 0.92,
        CatalogKind::Agent => 0.82,
        CatalogKind::Routine => 0.86,
        CatalogKind::Workflow => 0.84,
        CatalogKind::Prompt => 0.82,
        CatalogKind::Mcp => 0.88,
        CatalogKind::Automation => 0.72,
    }
}

fn kind_label(kind: CatalogKind) -> &'static str {
    match kind {
        CatalogKind::Agent => "Agent",
        CatalogKind::Automation => "Automation",
        CatalogKind::Mcp => "MCP",
        CatalogKind::Plugin => "Plugin",
        CatalogKind::Prompt => "Prompt",
        CatalogKind::Routine => "Routine",
        CatalogKind::Skill => "Skill",
        CatalogKind::Workflow => "Workflow",
    }
}

fn build_id(path: &Path, home_dir: &Path, name: &str) -> String {
    format!("{}::{}", display_path(path, home_dir), slugify(name))
}

fn display_path(path: &Path, home_dir: &Path) -> String {
    path.strip_prefix(home_dir)
        .map(|relative| format!("~/{}", relative.display()))
        .unwrap_or_else(|_| path.display().to_string())
}

fn workspace_display_name(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "workspace".into())
}

fn normalize_summary(value: String) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }

    slug.trim_matches('-').to_string()
}

fn dedupe_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for tag in tags.into_iter().filter(|tag| !tag.trim().is_empty()) {
        let key = tag.to_ascii_lowercase();
        if seen.insert(key) {
            deduped.push(tag);
        }
    }

    deduped
}

fn sanitize_entrypoint(value: &str) -> String {
    value
        .split('?')
        .next()
        .unwrap_or(value)
        .split('#')
        .next()
        .unwrap_or(value)
        .to_string()
}

fn component_depth(path: &Path) -> usize {
    path.components().count()
}

fn is_excluded_discovery_path(path: &Path, home_dir: &Path) -> bool {
    let canonical = fs::canonicalize(path).ok();

    EXCLUDED_DISCOVERY_DIRS.iter().any(|relative| {
        let excluded_root = home_dir.join(relative);
        path.starts_with(&excluded_root)
            || canonical
                .as_ref()
                .is_some_and(|resolved_path| resolved_path.starts_with(&excluded_root))
    })
}

fn expand_path(value: &str, home_dir: &Path) -> PathBuf {
    if value == "~" {
        home_dir.to_path_buf()
    } else if let Some(suffix) = value.strip_prefix("~/") {
        home_dir.join(suffix)
    } else {
        PathBuf::from(value)
    }
}

fn modified_at(path: &Path) -> Option<i64> {
    path.metadata()
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
}

fn try_read_to_string(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let metadata = path.metadata().ok()?;
    if metadata.len() > MAX_SUMMARY_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn try_read_json<T>(path: &Path) -> Option<T>
where
    T: for<'de> Deserialize<'de>,
{
    let content = try_read_to_string(path)?;
    serde_json::from_str(&content).ok()
}

fn try_read_json_value(path: &Path) -> Option<Value> {
    let content = try_read_to_string(path)?;
    serde_json::from_str(&content).ok()
}

fn load_marketplace_index(path: &Path) -> Option<HashMap<String, MarketplacePlugin>> {
    let marketplace = try_read_json::<MarketplaceIndex>(path)?;
    Some(
        marketplace
            .plugins
            .into_iter()
            .map(|plugin| (plugin.name.clone(), plugin))
            .collect(),
    )
}

fn open_readonly(path: &Path) -> AppResult<Option<Connection>> {
    if !path.exists() {
        return Ok(None);
    }

    Ok(Some(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?))
}

fn table_exists(connection: &Connection, table: &str) -> AppResult<bool> {
    let mut statement = connection
        .prepare("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    let count = statement.query_row([table], |row| row.get::<_, i64>(0))?;
    Ok(count > 0)
}

fn should_visit(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    !matches!(
        name.as_ref(),
        ".git"
            | ".idea"
            | ".next"
            | ".turbo"
            | "coverage"
            | "dist"
            | "node_modules"
            | "target"
            | "venv"
            | ".venv"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_item(kind: CatalogKind, confidence: f64, summary: Option<&str>) -> CatalogItem {
        CatalogItem {
            id: "~/workspace/package.json::demo".into(),
            kind,
            name: "demo".into(),
            display_name: "Demo".into(),
            summary: summary.map(|value| value.into()),
            path: "~/workspace/package.json".into(),
            origin: "workspace manifest".into(),
            group: "workspace".into(),
            tags: vec!["manifest".into()],
            version: None,
            category: Some("package-manifest".into()),
            updated_at: Some(100),
            provider: "local".into(),
            detector: "custom-rules".into(),
            entrypoint: None,
            confidence,
        }
    }

    #[test]
    fn build_id_uses_artifact_path_and_name() {
        let home = Path::new("/Users/test");
        let path = Path::new("/Users/test/workspace/package.json");

        assert_eq!(
            build_id(path, home, "My Demo Agent"),
            "~/workspace/package.json::my-demo-agent"
        );
    }

    #[test]
    fn merge_catalog_items_prefers_higher_confidence_and_merges_metadata() {
        let primary = sample_item(CatalogKind::Automation, 0.72, Some("Short note."));
        let mut secondary = sample_item(
            CatalogKind::Agent,
            0.84,
            Some("Longer summary for the detected agent artifact."),
        );
        secondary.provider = "openai".into();
        secondary.entrypoint = Some("~/workspace/main.py".into());
        secondary.tags = vec!["agent".into(), "openai".into()];
        secondary.updated_at = Some(240);

        let merged = merge_catalog_items(primary, secondary);

        assert_eq!(merged.kind, CatalogKind::Agent);
        assert_eq!(merged.provider, "openai");
        assert_eq!(merged.entrypoint.as_deref(), Some("~/workspace/main.py"));
        assert_eq!(merged.updated_at, Some(240));
        assert!(merged
            .summary
            .as_deref()
            .is_some_and(|value| value.contains("Longer summary")));
        assert!(merged.tags.iter().any(|tag| tag == "manifest"));
        assert!(merged.tags.iter().any(|tag| tag == "agent"));
        assert!(merged.tags.iter().any(|tag| tag == "openai"));
    }

    #[test]
    fn workspace_candidate_excludes_cloud_storage_subtree() {
        let nonce = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("agent-control-discovery-{nonce}"));
        let home = root.join("home");
        let excluded = home.join("Library/CloudStorage/Dropbox/project");
        let allowed = home.join("Documents/project");

        fs::create_dir_all(&excluded).unwrap();
        fs::create_dir_all(&allowed).unwrap();

        assert!(is_excluded_discovery_path(&excluded, &home));
        assert!(!is_excluded_discovery_path(&allowed, &home));
        assert!(!is_workspace_candidate(&excluded, &home));
        assert!(is_workspace_candidate(&allowed, &home));

        fs::remove_dir_all(root).unwrap();
    }
}
