use anyhow::Result;
use std::collections::HashMap;

use crate::db;

#[derive(Debug, Clone, Default)]
pub struct ConsolidationReport {
    pub subjects_scanned: usize,
    pub memories_scanned: usize,
    pub duplicates_removed: usize,
}

/// Run a conservative consolidation pass.
///
/// Strategy: exact dedupe only (same subject + type + normalized content).
/// Keeps the most recently updated memory and tombstones older duplicates.
pub fn run_consolidation_pass() -> Result<ConsolidationReport> {
    let conn = db::open()?;
    let subjects = db::search::list_subjects(&conn)?;

    let mut report = ConsolidationReport::default();

    for (subject, _count) in subjects {
        report.subjects_scanned += 1;

        // Pull a bounded set to avoid runaway work in one pass.
        let memories = db::search::by_subject(&conn, &subject, 300)?;
        report.memories_scanned += memories.len();

        // by_subject() already orders updated_at DESC, so first seen is keeper.
        let mut seen: HashMap<(String, String, String), String> = HashMap::new();

        for mem in memories {
            let normalized = mem
                .content
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            let key = (
                subject.clone(),
                mem.memory_type.to_string(),
                normalized,
            );

            if seen.contains_key(&key) {
                if db::memories::delete(&conn, &mem.id)? {
                    report.duplicates_removed += 1;
                }
            } else {
                seen.insert(key, mem.id.clone());
            }
        }
    }

    Ok(report)
}
