use rusqlite::params;
use uuid::Uuid;

use crate::db::Db;
use crate::error::AppResult;

use super::ManualSaveRequest;

const CONSOLIDATION_WINDOW_DAYS: i64 = 7;
const MAX_PROPOSALS_PER_EPISODE: usize = 3;

#[derive(Debug)]
struct DueEpisode {
    id: String,
    domain: String,
    title: String,
    vault_path: String,
    sensitivity: String,
}

/// Convert expiring episodes into governed fact/decision proposals. Weak
/// prose may yield no candidate; every surviving candidate still traverses
/// the deterministic gate and can never auto-apply.
pub fn propose_due(db: &Db) -> AppResult<i64> {
    super::index::ensure_tables(db)?;
    let horizon =
        chrono::Utc::now().date_naive() + chrono::Duration::days(CONSOLIDATION_WINDOW_DAYS);
    let episodes = db.with_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT m.id, m.domain, m.title, m.vault_path, m.sensitivity
             FROM memories m
             WHERE m.mem_type = 'episode'
               AND m.status = 'active'
               AND m.expires_at IS NOT NULL
               AND substr(m.expires_at, 1, 10) <= ?1
               AND NOT EXISTS (
                   SELECT 1 FROM episode_consolidations c WHERE c.episode_id = m.id
               )
             ORDER BY m.expires_at, m.updated_at",
        )?;
        let rows = statement
            .query_map(params![horizon.format("%Y-%m-%d").to_string()], |row| {
                Ok(DueEpisode {
                    id: row.get(0)?,
                    domain: row.get(1)?,
                    title: row.get(2)?,
                    vault_path: row.get(3)?,
                    sensitivity: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;

    let mut created = 0i64;
    for episode in episodes {
        let (content, _) = super::vault::read_file(&episode.vault_path)?;
        let Some((_, body)) = super::frontmatter::parse(&content) else {
            record(db, &episode.id, None, "needs_attention")?;
            continue;
        };
        let source = format!("consolidation:{}", episode.id);
        let candidates = super::importer::extract_candidates(
            &episode.title,
            &body,
            &episode.vault_path,
            &source,
        );
        if candidates.is_empty() {
            record(db, &episode.id, None, "no_candidates")?;
            continue;
        }

        let mut episode_created = 0usize;
        for mut candidate in candidates.into_iter().take(MAX_PROPOSALS_PER_EPISODE) {
            candidate.sensitivity = Some(episode.sensitivity.clone());
            if !candidate.tags.iter().any(|tag| tag == "consolidated") {
                candidate.tags.push("consolidated".to_string());
            }
            let request = ManualSaveRequest {
                domain: episode.domain.clone(),
                mem_type: candidate.mem_type,
                title: candidate.title,
                body: candidate.body,
                tags: candidate.tags,
                sensitivity: candidate.sensitivity,
                source: Some(source.clone()),
                confidence: candidate.confidence.map(|value| value.min(0.8)),
                valid_from: candidate.valid_from,
                valid_until: candidate.valid_until,
                stale_after_days: candidate.stale_after_days,
                expires: None,
                supersedes_id: candidate.supersedes_id,
            };
            match super::pipeline::process_consolidation_candidate(
                db,
                &request,
                &source,
                &episode.vault_path,
            ) {
                Ok(proposal) => {
                    record(db, &episode.id, Some(&proposal.id), "pending")?;
                    episode_created += 1;
                    created += 1;
                }
                Err(error) => {
                    log::warn!(
                        "episode consolidation candidate rejected for {}: {}",
                        episode.id,
                        error
                    );
                }
            }
        }
        if episode_created == 0 {
            record(db, &episode.id, None, "needs_attention")?;
        }
    }
    Ok(created)
}

fn record(db: &Db, episode_id: &str, proposal_id: Option<&str>, status: &str) -> AppResult<()> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO episode_consolidations (
                id, episode_id, proposal_id, status, created_at, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?5)",
            params![id, episode_id, proposal_id, status, now],
        )?;
        Ok(())
    })
}

pub fn refresh_for_proposal(db: &Db, proposal_id: &str, proposal_status: &str) -> AppResult<()> {
    let status = match proposal_status {
        "approved" | "auto_applied" => "approved",
        "discarded" => "discarded",
        _ => "pending",
    };
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE episode_consolidations
             SET status = ?1, updated_at = ?2 WHERE proposal_id = ?3",
            params![status, chrono::Utc::now().to_rfc3339(), proposal_id],
        )?;
        Ok(())
    })
}
