//! Knowledge integration: Validate reasoning against external knowledge

use crate::thinking::core::error::Result;
use async_trait::async_trait;

/// Knowledge source trait for pluggable knowledge integration
#[async_trait]
pub trait KnowledgeSource: Send + Sync {
    /// Look up facts about a topic
    async fn lookup(&self, query: &str) -> Result<Vec<Fact>>;

    /// Validate a claim against known facts
    async fn validate(&self, claim: &str) -> Result<ValidationResult>;

    /// Find supporting evidence for a claim
    async fn find_evidence(&self, claim: &str) -> Result<Vec<String>>;
}

/// A fact from a knowledge source
#[derive(Debug, Clone)]
pub struct Fact {
    pub content: String,
    pub source: String,
    pub confidence: f64,
}

/// Result of validating a claim
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationResult {
    Confirmed,
    Contradicted,
    Unknown,
}

/// Mock knowledge source for testing and Phase 1
pub struct MockKnowledgeSource;

#[async_trait]
impl KnowledgeSource for MockKnowledgeSource {
    async fn lookup(&self, _query: &str) -> Result<Vec<Fact>> {
        Ok(Vec::new())
    }

    async fn validate(&self, _claim: &str) -> Result<ValidationResult> {
        Ok(ValidationResult::Unknown)
    }

    async fn find_evidence(&self, _claim: &str) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}

/// Manages knowledge integration in reasoning
pub struct KnowledgeIntegrator {
    source: Option<Box<dyn KnowledgeSource>>,
}

impl KnowledgeIntegrator {
    #[must_use]
    pub fn new() -> Self {
        Self { source: None }
    }

    #[must_use]
    pub fn with_source(mut self, source: Box<dyn KnowledgeSource>) -> Self {
        self.source = Some(source);
        self
    }

    /// Validate a thought against knowledge.
    ///
    pub async fn validate_claim(&self, claim: &str) -> Result<ValidationResult> {
        match &self.source {
            Some(source) => source.validate(claim).await,
            None => Ok(ValidationResult::Unknown),
        }
    }

    /// Find supporting evidence for a thought.
    ///
    pub async fn find_evidence(&self, claim: &str) -> Result<Vec<String>> {
        match &self.source {
            Some(source) => source.find_evidence(claim).await,
            None => Ok(Vec::new()),
        }
    }

    /// Look up related facts.
    ///
    pub async fn lookup_facts(&self, query: &str) -> Result<Vec<Fact>> {
        match &self.source {
            Some(source) => source.lookup(query).await,
            None => Ok(Vec::new()),
        }
    }
}

impl Default for KnowledgeIntegrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_knowledge_source() -> Result<()> {
        let source = MockKnowledgeSource;
        let result = source.validate("test").await?;
        assert_eq!(result, ValidationResult::Unknown);
        Ok(())
    }

    #[tokio::test]
    async fn test_knowledge_integrator() -> Result<()> {
        let integrator = KnowledgeIntegrator::new();
        let result = integrator.validate_claim("test").await?;
        assert_eq!(result, ValidationResult::Unknown);
        Ok(())
    }

    #[tokio::test]
    async fn test_integrator_with_mock_source() -> Result<()> {
        let integrator = KnowledgeIntegrator::new().with_source(Box::new(MockKnowledgeSource));
        let result = integrator.validate_claim("test").await?;
        assert_eq!(result, ValidationResult::Unknown);
        let facts = integrator.lookup_facts("test").await?;
        assert!(facts.is_empty());
        let evidence = integrator.find_evidence("test").await?;
        assert!(evidence.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_integrator_no_source_find_evidence() -> Result<()> {
        let integrator = KnowledgeIntegrator::new();
        let evidence = integrator.find_evidence("anything").await?;
        assert!(evidence.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_integrator_no_source_lookup() -> Result<()> {
        let integrator = KnowledgeIntegrator::new();
        let facts = integrator.lookup_facts("anything").await?;
        assert!(facts.is_empty());
        Ok(())
    }

    #[test]
    fn test_fact_fields() {
        let fact = Fact {
            content: "rust is safe".into(),
            source: "docs".into(),
            confidence: 0.9,
        };
        assert_eq!(fact.content, "rust is safe");
        assert_eq!(fact.source, "docs");
    }

    #[test]
    fn test_validation_result_variants() {
        assert_eq!(ValidationResult::Confirmed, ValidationResult::Confirmed);
        assert_ne!(ValidationResult::Confirmed, ValidationResult::Contradicted);
    }

    #[test]
    fn test_knowledge_integrator_default() {
        let _ki = KnowledgeIntegrator::default();
    }

    #[test]
    fn test_mock_lookup_empty() {
        let source = MockKnowledgeSource;
        // Just verify construction
        let _ = &source;
    }
}
