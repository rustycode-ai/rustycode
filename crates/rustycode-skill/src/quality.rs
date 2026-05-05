use crate::types::{QualityGrade, SkillQuality};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityRecord {
    pub skill_id: String,
    pub quality: SkillQuality,
    pub usage_count: u32,
    pub last_used: Option<String>,
}

pub struct QualityScorer {
    records: HashMap<String, QualityRecord>,
    storage_dir: Option<PathBuf>,
}

impl QualityScorer {
    #[allow(clippy::missing_const_for_fn)]
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            storage_dir: None,
        }
    }

    pub fn with_storage_dir(mut self, dir: PathBuf) -> Self {
        self.storage_dir = Some(dir);
        self
    }

    pub fn compute_score(
        &self,
        skill_id: &str,
        telemetry: f64,
        graph: f64,
        intake: f64,
        routing: f64,
    ) -> SkillQuality {
        let existing = self.records.get(skill_id);

        let (t, g, i, r) = existing.map_or_else(
            || {
                (
                    telemetry.clamp(0.0, 1.0),
                    graph.clamp(0.0, 1.0),
                    intake.clamp(0.0, 1.0),
                    routing.clamp(0.0, 1.0),
                )
            },
            |rec| {
                let alpha = 0.3;
                (
                    Self::ema(rec.quality.telemetry_score, telemetry, alpha),
                    Self::ema(rec.quality.graph_score, graph, alpha),
                    Self::ema(rec.quality.intake_score, intake, alpha),
                    Self::ema(rec.quality.routing_score, routing, alpha),
                )
            },
        );

        SkillQuality::new(t, g, i, r)
    }

    #[allow(clippy::expect_used)]
    pub fn observe_usage(&mut self, skill_id: &str) -> SkillQuality {
        let (usage_count, prev_t, prev_g, prev_i, prev_r) = {
            let rec = self
                .records
                .entry(skill_id.to_string())
                .or_insert_with(|| QualityRecord {
                    skill_id: skill_id.to_string(),
                    quality: SkillQuality::default_new(),
                    usage_count: 0,
                    last_used: None,
                });

            rec.usage_count += 1;
            rec.last_used = Some(chrono::Utc::now().to_rfc3339());

            (
                rec.usage_count,
                rec.quality.telemetry_score,
                rec.quality.graph_score,
                rec.quality.intake_score,
                rec.quality.routing_score,
            )
        };

        #[allow(clippy::cast_precision_loss)]
        let usage_bonus = f64::from(usage_count).ln() / 10.0;
        let updated = self.compute_score(skill_id, prev_t + usage_bonus, prev_g, prev_i, prev_r);

        self.records
            .get_mut(skill_id)
            .expect("just inserted")
            .quality = updated.clone();

        updated
    }

    pub fn observe_score(&mut self, skill_id: &str, quality: SkillQuality) {
        let rec = self
            .records
            .entry(skill_id.to_string())
            .or_insert_with(|| QualityRecord {
                skill_id: skill_id.to_string(),
                quality: quality.clone(),
                usage_count: 0,
                last_used: None,
            });
        rec.quality = quality;
    }

    pub fn quality(&self, skill_id: &str) -> Option<&SkillQuality> {
        self.records.get(skill_id).map(|r| &r.quality)
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn grade(&self, skill_id: &str) -> QualityGrade {
        self.records
            .get(skill_id)
            .map_or(QualityGrade::Fair, |r| r.quality.grade())
    }

    pub fn record(&self, skill_id: &str) -> Option<&QualityRecord> {
        self.records.get(skill_id)
    }

    pub fn all_records(&self) -> Vec<&QualityRecord> {
        self.records.values().collect()
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    pub fn load_from_dir(&mut self, dir: &Path) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read quality file: {}", path.display()))?;
                let record: QualityRecord = serde_json::from_str(&content)
                    .with_context(|| format!("Failed to parse quality file: {}", path.display()))?;
                self.records.insert(record.skill_id.clone(), record);
            }
        }

        Ok(())
    }

    pub fn persist(&self) -> Result<()> {
        let Some(dir) = &self.storage_dir else {
            return Ok(());
        };

        std::fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create quality dir: {}", dir.display()))?;

        for record in self.records.values() {
            let slug = record.skill_id.replace(['/', ' ', '\\'], "_");
            let path = dir.join(format!("{slug}.json"));
            let content = serde_json::to_string_pretty(record)?;
            std::fs::write(&path, content)
                .with_context(|| format!("Failed to write quality file: {}", path.display()))?;
        }

        Ok(())
    }

    pub fn reset(&mut self) {
        self.records.clear();
    }

    fn ema(previous: f64, current: f64, alpha: f64) -> f64 {
        alpha
            .mul_add(current, (1.0 - alpha) * previous)
            .clamp(0.0, 1.0)
    }
}

impl Default for QualityScorer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_scorer_is_empty() {
        let s = QualityScorer::new();
        assert_eq!(s.record_count(), 0);
        assert!(s.quality("test").is_none());
    }

    #[test]
    fn default_scorer_is_empty() {
        let s = QualityScorer::default();
        assert_eq!(s.record_count(), 0);
    }

    #[test]
    fn compute_score_clamps() {
        let s = QualityScorer::new();
        let q = s.compute_score("test", 2.0, -1.0, 0.5, 1.5);
        assert!((q.telemetry_score - 1.0).abs() < f64::EPSILON);
        assert!((q.graph_score - 0.0).abs() < f64::EPSILON);
        assert!((q.routing_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_score_with_existing_uses_ema() {
        let mut s = QualityScorer::new();
        s.observe_score("test", SkillQuality::new(0.5, 0.5, 0.5, 0.5));

        let q = s.compute_score("test", 1.0, 1.0, 1.0, 1.0);
        assert!(q.telemetry_score > 0.5);
        assert!(q.telemetry_score < 1.0);
    }

    #[test]
    fn compute_score_new_skill_no_ema() {
        let s = QualityScorer::new();
        let q = s.compute_score("new", 0.8, 0.7, 0.6, 0.5);
        assert!((q.telemetry_score - 0.8).abs() < f64::EPSILON);
        assert!((q.graph_score - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn observe_usage_increments() {
        let mut s = QualityScorer::new();
        let _q = s.observe_usage("test");
        assert_eq!(s.record("test").unwrap().usage_count, 1);
        assert!(s.record("test").unwrap().last_used.is_some());
    }

    #[test]
    fn observe_usage_multiple() {
        let mut s = QualityScorer::new();
        s.observe_usage("test");
        s.observe_usage("test");
        s.observe_usage("test");
        assert_eq!(s.record("test").unwrap().usage_count, 3);
    }

    #[test]
    fn observe_score_updates() {
        let mut s = QualityScorer::new();
        let q = SkillQuality::new(0.9, 0.8, 0.7, 0.6);
        s.observe_score("test", q.clone());
        let stored = s.quality("test").unwrap();
        assert!((stored.telemetry_score - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn get_grade_returns_quality_grade() {
        let mut s = QualityScorer::new();
        s.observe_score("test", SkillQuality::new(0.9, 0.9, 0.9, 0.9));
        assert_eq!(s.grade("test"), QualityGrade::Excellent);
    }

    #[test]
    fn get_grade_missing_defaults_to_fair() {
        let s = QualityScorer::new();
        assert_eq!(s.grade("missing"), QualityGrade::Fair);
    }

    #[test]
    fn persist_and_load() {
        let dir = std::env::temp_dir().join(format!("rustycode-quality-{}", uuid::Uuid::new_v4()));
        let mut s = QualityScorer::new().with_storage_dir(dir.clone());
        s.observe_score("my-skill", SkillQuality::new(0.9, 0.8, 0.7, 0.6));
        s.persist().unwrap();

        let mut s2 = QualityScorer::new();
        s2.load_from_dir(&dir).unwrap();
        assert_eq!(s2.record_count(), 1);
        let q = s2.quality("my-skill").unwrap();
        assert!((q.telemetry_score - 0.9).abs() < f64::EPSILON);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_nonexistent_dir() {
        let mut s = QualityScorer::new();
        let result = s.load_from_dir(Path::new("/nonexistent"));
        assert!(result.is_ok());
    }

    #[test]
    fn persist_without_storage_dir_is_noop() {
        let s = QualityScorer::new();
        assert!(s.persist().is_ok());
    }

    #[test]
    fn reset_clears_records() {
        let mut s = QualityScorer::new();
        s.observe_score("test", SkillQuality::new(0.5, 0.5, 0.5, 0.5));
        assert_eq!(s.record_count(), 1);
        s.reset();
        assert_eq!(s.record_count(), 0);
    }

    #[test]
    fn all_records_returns_all() {
        let mut s = QualityScorer::new();
        s.observe_score("a", SkillQuality::new(0.5, 0.5, 0.5, 0.5));
        s.observe_score("b", SkillQuality::new(0.6, 0.6, 0.6, 0.6));
        assert_eq!(s.all_records().len(), 2);
    }

    #[test]
    fn quality_grade_thresholds() {
        assert_eq!(QualityGrade::from_score(0.85), QualityGrade::Excellent);
        assert_eq!(QualityGrade::from_score(0.65), QualityGrade::Good);
        assert_eq!(QualityGrade::from_score(0.45), QualityGrade::Fair);
        assert_eq!(QualityGrade::from_score(0.25), QualityGrade::Poor);
        assert_eq!(QualityGrade::from_score(0.15), QualityGrade::Critical);
    }
}
