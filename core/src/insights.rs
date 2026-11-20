use crate::simulation::SorobanResources;
use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────────────────

/// Severity level for an optimisation insight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// A single actionable insight produced by the analysis engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Insight {
    pub severity: Severity,
    pub rule: String,
    pub message: String,
    pub suggested_fix: String,
}

/// Complete insights report returned alongside resource metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InsightsReport {
    /// Weighted efficiency score in the range 0–100.
    pub efficiency_score: u32,
    /// Individual insights (may be empty when the contract is well-optimised).
    pub insights: Vec<Insight>,
}

// ── Rule trait ─────────────────────────────────────────────────────────────────

/// Extensible trait — implement this to add new heuristic rules without
/// touching existing code.
pub trait InsightRule: Send + Sync {
    /// Unique identifier for this rule (e.g. `"storage_efficiency"`).
    fn name(&self) -> &str;

    /// Evaluate the rule against a resource footprint and return zero or more
    /// insights.
    fn evaluate(&self, resources: &SorobanResources) -> Vec<Insight>;
}

// ── Built-in rules ────────────────────────────────────────────────────────────

/// Warn when ledger writes exceed the full transaction-size baseline.
///
/// This heuristic uses a 1:1 ratio: above this point, bytes written to the
/// ledger outweigh the transaction data submitted.
const STORAGE_WRITE_WARNING_RATIO: f64 = 1.0;

/// Mark storage writes critical once they exceed twice the transaction-size baseline.
///
/// The 2:1 ratio is the escalation point above the warning threshold, indicating
/// that ledger writes are more than double the submitted transaction data. These
/// ratios are analysis heuristics, not Soroban protocol limits.
const STORAGE_WRITE_CRITICAL_RATIO: f64 = 2.0;

/// Flags disproportionately high ledger write bytes relative to overall
/// transaction data, suggesting inefficient storage patterns.
pub struct StorageEfficiencyRule;

impl InsightRule for StorageEfficiencyRule {
    fn name(&self) -> &str {
        "storage_efficiency"
    }

    fn evaluate(&self, r: &SorobanResources) -> Vec<Insight> {
        let mut out = Vec::new();

        // Skip if there's no meaningful data to analyse.
        if r.transaction_size_bytes == 0 {
            return out;
        }

        let write_ratio = r.ledger_write_bytes as f64 / r.transaction_size_bytes as f64;

        if write_ratio > STORAGE_WRITE_CRITICAL_RATIO {
            out.push(Insight {
                severity: Severity::Critical,
                rule: self.name().to_string(),
                message: format!(
                    "Ledger write bytes ({}) are {:.1}x the transaction size ({}) \
                     — extremely write-heavy",
                    r.ledger_write_bytes, write_ratio, r.transaction_size_bytes
                ),
                suggested_fix: "Use temporary storage (TTL entries) for ephemeral data \
                                and batch writes where possible."
                    .to_string(),
            });
        } else if write_ratio > STORAGE_WRITE_WARNING_RATIO {
            out.push(Insight {
                severity: Severity::Warning,
                rule: self.name().to_string(),
                message: format!(
                    "Ledger write bytes ({}) exceed transaction size ({}) \
                     — consider reviewing storage layout",
                    r.ledger_write_bytes, r.transaction_size_bytes
                ),
                suggested_fix:
                    "Consolidate writes into fewer ledger keys or use compact serialization."
                        .to_string(),
            });
        }

        out
    }
}

/// Detects high CPU usage with relatively low ledger activity, indicating
/// computation-heavy logic that may benefit from off-chain pre-computation.
pub struct InstructionDensityRule;

impl InsightRule for InstructionDensityRule {
    fn name(&self) -> &str {
        "instruction_density"
    }

    fn evaluate(&self, r: &SorobanResources) -> Vec<Insight> {
        let mut out = Vec::new();

        let total_ledger = r.ledger_read_bytes.saturating_add(r.ledger_write_bytes);

        // High CPU with low ledger I/O → pure compute workload.
        if r.cpu_instructions > 50_000_000 && total_ledger < 1_024 {
            out.push(Insight {
                severity: Severity::Critical,
                rule: self.name().to_string(),
                message: format!(
                    "Very high CPU ({} instructions) with minimal ledger I/O ({} bytes) \
                     — heavy computation detected",
                    r.cpu_instructions, total_ledger
                ),
                suggested_fix: "Cache intermediate results in persistent storage or move \
                                complex calculations off-chain with on-chain verification."
                    .to_string(),
            });
        } else if r.cpu_instructions > 10_000_000 && total_ledger < 2_048 {
            out.push(Insight {
                severity: Severity::Warning,
                rule: self.name().to_string(),
                message: format!(
                    "High CPU ({} instructions) relative to ledger activity ({} bytes) \
                     — consider optimising hot loops",
                    r.cpu_instructions, total_ledger
                ),
                suggested_fix:
                    "Profile the contract to identify hot loops; consider lookup tables \
                     or pre-computed values."
                        .to_string(),
            });
        }

        out
    }
}

/// Flags transactions with a large footprint (many ledger keys), which
/// increases base fees and contention risk.
pub struct FootprintBloatRule;

impl InsightRule for FootprintBloatRule {
    fn name(&self) -> &str {
        "footprint_bloat"
    }

    fn evaluate(&self, r: &SorobanResources) -> Vec<Insight> {
        let mut out = Vec::new();

        // Heuristic: average ledger key ≈ 40–80 bytes.  We estimate the key
        // count from the total footprint size.
        let estimated_keys = r.ledger_read_bytes.saturating_add(r.ledger_write_bytes) / 60;

        if estimated_keys > 20 {
            out.push(Insight {
                severity: Severity::Critical,
                rule: self.name().to_string(),
                message: format!(
                    "Estimated footprint contains ~{} ledger keys — very large transaction",
                    estimated_keys
                ),
                suggested_fix: "Split the operation into smaller batches or reduce the \
                                number of distinct storage keys accessed per invocation."
                    .to_string(),
            });
        } else if estimated_keys > 10 {
            out.push(Insight {
                severity: Severity::Warning,
                rule: self.name().to_string(),
                message: format!(
                    "Estimated footprint contains ~{} ledger keys — above recommended threshold",
                    estimated_keys
                ),
                suggested_fix: "Consider consolidating related data into fewer keys \
                                (e.g., a single Map entry instead of many individual keys)."
                    .to_string(),
            });
        }

        out
    }
}

/// Flags high RAM usage which may push against per-transaction memory limits.
pub struct MemoryPressureRule;

impl InsightRule for MemoryPressureRule {
    fn name(&self) -> &str {
        "memory_pressure"
    }

    fn evaluate(&self, r: &SorobanResources) -> Vec<Insight> {
        let mut out = Vec::new();

        if r.ram_bytes > 20 * 1024 * 1024 {
            out.push(Insight {
                severity: Severity::Critical,
                rule: self.name().to_string(),
                message: format!(
                    "RAM usage ({} bytes / {:.1} MiB) is very high — \
                     approaching protocol memory limits",
                    r.ram_bytes,
                    r.ram_bytes as f64 / (1024.0 * 1024.0)
                ),
                suggested_fix: "Reduce in-memory data structures; process data in \
                                streaming fashion rather than loading everything at once."
                    .to_string(),
            });
        } else if r.ram_bytes > 5 * 1024 * 1024 {
            out.push(Insight {
                severity: Severity::Warning,
                rule: self.name().to_string(),
                message: format!(
                    "RAM usage ({} bytes / {:.1} MiB) is elevated",
                    r.ram_bytes,
                    r.ram_bytes as f64 / (1024.0 * 1024.0)
                ),
                suggested_fix:
                    "Review large allocations; consider lazy initialization or smaller buffers."
                        .to_string(),
            });
        }

        out
    }
}

// ── Engine ────────────────────────────────────────────────────────────────────

/// The insights engine holds a set of rules and evaluates them against resource
/// metrics to produce an `InsightsReport`.
pub struct InsightsEngine {
    rules: Vec<Box<dyn InsightRule>>,
    /// When true, halts rule evaluation immediately after finding the first Critical insight.
    pub stop_after_critical: bool,
}

impl Clone for InsightsEngine {
    fn clone(&self) -> Self {
        let mut engine = Self::new();
        engine.stop_after_critical = self.stop_after_critical;
        engine
    }
}

impl InsightsEngine {
    /// Create an engine pre-loaded with all built-in rules.
    pub fn new() -> Self {
        Self {
            rules: vec![
                Box::new(StorageEfficiencyRule),
                Box::new(InstructionDensityRule),
                Box::new(FootprintBloatRule),
                Box::new(MemoryPressureRule),
            ],
            stop_after_critical: false,
        }
    }

    /// Builder method to configure early exit after finding the first critical insight.
    pub fn with_stop_after_critical(mut self, stop: bool) -> Self {
        self.stop_after_critical = stop;
        self
    }

    /// Returns whether the engine short-circuits evaluation after encountering a critical insight.
    pub fn stop_after_critical(&self) -> bool {
        self.stop_after_critical
    }

    /// Set whether the engine short-circuits evaluation after encountering a critical insight.
    pub fn set_stop_after_critical(&mut self, stop: bool) {
        self.stop_after_critical = stop;
    }

    /// Add a custom rule at runtime.
    #[allow(dead_code)]
    pub fn add_rule(&mut self, rule: Box<dyn InsightRule>) {
        self.rules.push(rule);
    }

    /// Run all rules and compute the efficiency score.
    ///
    /// When `stop_after_critical` is enabled, rule evaluation short-circuits
    /// immediately after the first `Severity::Critical` insight is encountered.
    pub fn analyze(&self, resources: &SorobanResources) -> InsightsReport {
        let mut insights = Vec::new();

        for rule in &self.rules {
            for insight in rule.evaluate(resources) {
                let is_critical = insight.severity == Severity::Critical;
                insights.push(insight);
                if self.stop_after_critical && is_critical {
                    break;
                }
            }
            if self.stop_after_critical && insights.iter().any(|i| i.severity == Severity::Critical)
            {
                break;
            }
        }

        let efficiency_score = Self::compute_efficiency_score(resources, &insights);

        InsightsReport {
            efficiency_score,
            insights,
        }
    }

    /// Weighted efficiency score (1–100).
    ///
    /// Starts at 100 and deducts points for:
    /// - Each Critical insight: −20  (penalty threshold: 5 criticals → floor)
    /// - Each Warning insight:  −10  (penalty threshold: 10 warnings → floor)
    /// - Each Info insight:     −3
    /// - High absolute resource usage (graduated penalties, max −30 total):
    ///   - CPU > 50 M instructions: −10; CPU > 10 M: −5
    ///   - RAM > 20 MiB:            −10; RAM >  5 MiB: −5
    ///   - Ledger I/O > 100 KiB:   −10; Ledger I/O > 50 KiB: −5
    ///
    /// The minimum returned score is **1** for any contract that executed
    /// successfully.  A score of 0 is never produced; that value is reserved
    /// to signal a failed or un-scored invocation so callers can distinguish
    /// "extremely inefficient but ran" from "did not run".
    fn compute_efficiency_score(resources: &SorobanResources, insights: &[Insight]) -> u32 {
        let mut score: i32 = 100;

        // Deduct for insight severity.
        for insight in insights {
            match insight.severity {
                Severity::Critical => score -= 20,
                Severity::Warning => score -= 10,
                Severity::Info => score -= 3,
            }
        }

        // Graduated penalties for absolute resource consumption.

        // CPU: mild penalty above 10M, heavier above 50M.
        if resources.cpu_instructions > 50_000_000 {
            score -= 10;
        } else if resources.cpu_instructions > 10_000_000 {
            score -= 5;
        }

        // RAM: penalty above 5 MiB.
        if resources.ram_bytes > 20 * 1024 * 1024 {
            score -= 10;
        } else if resources.ram_bytes > 5 * 1024 * 1024 {
            score -= 5;
        }

        // Ledger I/O: penalty for heavy readers/writers.
        let total_ledger = resources
            .ledger_read_bytes
            .saturating_add(resources.ledger_write_bytes);
        if total_ledger > 100 * 1024 {
            score -= 10;
        } else if total_ledger > 50 * 1024 {
            score -= 5;
        }

        // Floor at 1: a contract that executed successfully always receives a
        // non-zero score.  Callers can use 0 to signal a failed invocation.
        score.clamp(1, 100) as u32
    }
}

impl Default for InsightsEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_resources() -> SorobanResources {
        SorobanResources {
            cpu_instructions: 100_000,
            ram_bytes: 1_024,
            ledger_read_bytes: 256,
            ledger_write_bytes: 128,
            transaction_size_bytes: 512,
        }
    }

    // ── Efficiency score ──────────────────────────────────────────────────

    #[test]
    fn test_perfect_score_for_minimal_resources() {
        let engine = InsightsEngine::new();
        let report = engine.analyze(&minimal_resources());
        assert_eq!(report.efficiency_score, 100);
        assert!(report.insights.is_empty());
    }

    #[test]
    fn test_score_never_below_zero() {
        let engine = InsightsEngine::new();
        let r = SorobanResources {
            cpu_instructions: 500_000_000,
            ram_bytes: 50 * 1024 * 1024,
            ledger_read_bytes: 200 * 1024,
            ledger_write_bytes: 200 * 1024,
            transaction_size_bytes: 1_024,
        };
        let report = engine.analyze(&r);
        assert!(report.efficiency_score <= 100);
    }

    #[test]
    fn test_score_capped_at_100() {
        let engine = InsightsEngine::new();
        let report = engine.analyze(&SorobanResources::default());
        assert!(report.efficiency_score <= 100);
    }

    // ── Stop after critical (short-circuit) ────────────────────────────────

    #[test]
    fn test_stop_after_critical_disabled_by_default() {
        let engine = InsightsEngine::new();
        assert!(!engine.stop_after_critical);
        assert!(!engine.stop_after_critical());
    }

    #[test]
    fn test_stop_after_critical_builder_and_setter() {
        let mut engine = InsightsEngine::new().with_stop_after_critical(true);
        assert!(engine.stop_after_critical());

        engine.set_stop_after_critical(false);
        assert!(!engine.stop_after_critical());

        let cloned = engine.with_stop_after_critical(true).clone();
        assert!(cloned.stop_after_critical());
    }

    struct MockRule {
        name: &'static str,
        insights: Vec<Insight>,
        evaluated: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl InsightRule for MockRule {
        fn name(&self) -> &str {
            self.name
        }

        fn evaluate(&self, _r: &SorobanResources) -> Vec<Insight> {
            self.evaluated
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.insights.clone()
        }
    }

    #[test]
    fn test_stop_after_critical_short_circuits_rule_evaluation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let eval_counter_1 = Arc::new(AtomicUsize::new(0));
        let eval_counter_2 = Arc::new(AtomicUsize::new(0));

        let critical_insight = Insight {
            severity: Severity::Critical,
            rule: "mock_critical".to_string(),
            message: "Critical issue found".to_string(),
            suggested_fix: "Fix immediately".to_string(),
        };

        let warning_insight = Insight {
            severity: Severity::Warning,
            rule: "mock_warning".to_string(),
            message: "Warning issue found".to_string(),
            suggested_fix: "Consider fixing".to_string(),
        };

        // Case 1: stop_after_critical = false (evaluates all rules)
        let engine_without_stop = InsightsEngine {
            rules: vec![
                Box::new(MockRule {
                    name: "rule_1",
                    insights: vec![critical_insight.clone()],
                    evaluated: Arc::clone(&eval_counter_1),
                }),
                Box::new(MockRule {
                    name: "rule_2",
                    insights: vec![warning_insight.clone()],
                    evaluated: Arc::clone(&eval_counter_2),
                }),
            ],
            stop_after_critical: false,
        };

        let report_without_stop = engine_without_stop.analyze(&minimal_resources());
        assert_eq!(eval_counter_1.load(Ordering::SeqCst), 1);
        assert_eq!(eval_counter_2.load(Ordering::SeqCst), 1);
        assert_eq!(report_without_stop.insights.len(), 2);

        // Reset counters
        eval_counter_1.store(0, Ordering::SeqCst);
        eval_counter_2.store(0, Ordering::SeqCst);

        // Case 2: stop_after_critical = true (short-circuits after rule 1)
        let engine_with_stop = InsightsEngine {
            rules: vec![
                Box::new(MockRule {
                    name: "rule_1",
                    insights: vec![critical_insight],
                    evaluated: Arc::clone(&eval_counter_1),
                }),
                Box::new(MockRule {
                    name: "rule_2",
                    insights: vec![warning_insight],
                    evaluated: Arc::clone(&eval_counter_2),
                }),
            ],
            stop_after_critical: true,
        };

        let report_with_stop = engine_with_stop.analyze(&minimal_resources());
        assert_eq!(eval_counter_1.load(Ordering::SeqCst), 1);
        assert_eq!(
            eval_counter_2.load(Ordering::SeqCst),
            0,
            "rule 2 should not be evaluated after critical insight"
        );
        assert_eq!(report_with_stop.insights.len(), 1);
        assert_eq!(report_with_stop.insights[0].severity, Severity::Critical);
    }

    #[test]
    fn test_stop_after_critical_does_not_stop_on_warnings_or_info() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter_1 = Arc::new(AtomicUsize::new(0));
        let counter_2 = Arc::new(AtomicUsize::new(0));

        let warning = Insight {
            severity: Severity::Warning,
            rule: "w".to_string(),
            message: "w".to_string(),
            suggested_fix: "w".to_string(),
        };

        let info = Insight {
            severity: Severity::Info,
            rule: "i".to_string(),
            message: "i".to_string(),
            suggested_fix: "i".to_string(),
        };

        let engine = InsightsEngine {
            rules: vec![
                Box::new(MockRule {
                    name: "r1",
                    insights: vec![warning],
                    evaluated: Arc::clone(&counter_1),
                }),
                Box::new(MockRule {
                    name: "r2",
                    insights: vec![info],
                    evaluated: Arc::clone(&counter_2),
                }),
            ],
            stop_after_critical: true,
        };

        let report = engine.analyze(&minimal_resources());
        assert_eq!(counter_1.load(Ordering::SeqCst), 1);
        assert_eq!(counter_2.load(Ordering::SeqCst), 1);
        assert_eq!(report.insights.len(), 2);
    }

    #[test]
    fn test_stop_after_critical_with_builtin_rules() {
        // High CPU (triggers InstructionDensityRule critical) and high write ratio (triggers StorageEfficiencyRule critical)
        let r = SorobanResources {
            cpu_instructions: 60_000_000,
            ram_bytes: 1_024,
            ledger_read_bytes: 100,
            ledger_write_bytes: 5_000,
            transaction_size_bytes: 1_024,
        };

        let full_report = InsightsEngine::new().analyze(&r);
        assert!(
            full_report.insights.len() >= 2,
            "both rules should trigger critical insights"
        );

        let early_report = InsightsEngine::new()
            .with_stop_after_critical(true)
            .analyze(&r);
        assert_eq!(
            early_report.insights.len(),
            1,
            "only the first critical insight should be returned"
        );
        assert_eq!(early_report.insights[0].severity, Severity::Critical);
    }

    // ── Storage efficiency rule ───────────────────────────────────────────

    #[test]
    fn test_storage_efficiency_no_warning_when_balanced() {
        let rule = StorageEfficiencyRule;
        let r = SorobanResources {
            ledger_write_bytes: 400,
            transaction_size_bytes: 1_024,
            ..Default::default()
        };
        assert!(rule.evaluate(&r).is_empty());
    }

    #[test]
    fn test_storage_efficiency_warning_when_writes_exceed_tx_size() {
        let rule = StorageEfficiencyRule;
        let r = SorobanResources {
            ledger_write_bytes: 2_000,
            transaction_size_bytes: 1_024,
            ..Default::default()
        };
        let insights = rule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
        assert_eq!(insights[0].rule, "storage_efficiency");
    }

    #[test]
    fn test_storage_efficiency_critical_when_writes_double_tx_size() {
        let rule = StorageEfficiencyRule;
        let r = SorobanResources {
            ledger_write_bytes: 5_000,
            transaction_size_bytes: 1_024,
            ..Default::default()
        };
        let insights = rule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Critical);
    }

    #[test]
    fn test_storage_efficiency_skips_zero_tx_size() {
        let rule = StorageEfficiencyRule;
        let r = SorobanResources {
            ledger_write_bytes: 5_000,
            transaction_size_bytes: 0,
            ..Default::default()
        };
        assert!(rule.evaluate(&r).is_empty());
    }

    // ── Instruction density rule ──────────────────────────────────────────

    #[test]
    fn test_instruction_density_no_warning_when_balanced() {
        let rule = InstructionDensityRule;
        let r = SorobanResources {
            cpu_instructions: 5_000_000,
            ledger_read_bytes: 4_096,
            ledger_write_bytes: 2_048,
            ..Default::default()
        };
        assert!(rule.evaluate(&r).is_empty());
    }

    #[test]
    fn test_instruction_density_warning_high_cpu_low_ledger() {
        let rule = InstructionDensityRule;
        let r = SorobanResources {
            cpu_instructions: 15_000_000,
            ledger_read_bytes: 512,
            ledger_write_bytes: 256,
            ..Default::default()
        };
        let insights = rule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
        assert_eq!(insights[0].rule, "instruction_density");
    }

    #[test]
    fn test_instruction_density_critical_very_high_cpu() {
        let rule = InstructionDensityRule;
        let r = SorobanResources {
            cpu_instructions: 80_000_000,
            ledger_read_bytes: 256,
            ledger_write_bytes: 128,
            ..Default::default()
        };
        let insights = rule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Critical);
    }

    // ── Footprint bloat rule ──────────────────────────────────────────────

    #[test]
    fn test_footprint_bloat_no_warning_few_keys() {
        let rule = FootprintBloatRule;
        let r = SorobanResources {
            ledger_read_bytes: 256,
            ledger_write_bytes: 128,
            ..Default::default()
        };
        assert!(rule.evaluate(&r).is_empty());
    }

    #[test]
    fn test_footprint_bloat_warning_above_10_keys() {
        let rule = FootprintBloatRule;
        // ~11 estimated keys: (11 * 60) = 660 bytes
        let r = SorobanResources {
            ledger_read_bytes: 400,
            ledger_write_bytes: 300,
            ..Default::default()
        };
        let insights = rule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
        assert_eq!(insights[0].rule, "footprint_bloat");
    }

    #[test]
    fn test_footprint_bloat_critical_above_20_keys() {
        let rule = FootprintBloatRule;
        // ~25 estimated keys: 25 * 60 = 1500 bytes
        let r = SorobanResources {
            ledger_read_bytes: 1_000,
            ledger_write_bytes: 500,
            ..Default::default()
        };
        let insights = rule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Critical);
    }

    // ── Memory pressure rule ──────────────────────────────────────────────

    #[test]
    fn test_memory_pressure_no_warning_low_ram() {
        let rule = MemoryPressureRule;
        let r = SorobanResources {
            ram_bytes: 1_024 * 1_024,
            ..Default::default()
        };
        assert!(rule.evaluate(&r).is_empty());
    }

    #[test]
    fn test_memory_pressure_warning_elevated_ram() {
        let rule = MemoryPressureRule;
        let r = SorobanResources {
            ram_bytes: 10 * 1024 * 1024,
            ..Default::default()
        };
        let insights = rule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
        assert_eq!(insights[0].rule, "memory_pressure");
    }

    #[test]
    fn test_memory_pressure_critical_very_high_ram() {
        let rule = MemoryPressureRule;
        let r = SorobanResources {
            ram_bytes: 30 * 1024 * 1024,
            ..Default::default()
        };
        let insights = rule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Critical);
    }

    // ── Custom rule extensibility ─────────────────────────────────────────

    struct AlwaysWarnRule;

    impl InsightRule for AlwaysWarnRule {
        fn name(&self) -> &str {
            "always_warn"
        }

        fn evaluate(&self, _resources: &SorobanResources) -> Vec<Insight> {
            vec![Insight {
                severity: Severity::Info,
                rule: self.name().to_string(),
                message: "Custom rule triggered".to_string(),
                suggested_fix: "No action needed".to_string(),
            }]
        }
    }

    #[test]
    fn test_custom_rule_added_and_evaluated() {
        let mut engine = InsightsEngine::new();
        engine.add_rule(Box::new(AlwaysWarnRule));
        let report = engine.analyze(&minimal_resources());
        assert!(report.insights.iter().any(|i| i.rule == "always_warn"));
    }

    // ── Serialization ─────────────────────────────────────────────────────

    #[test]
    fn test_insights_report_serialization() {
        let report = InsightsReport {
            efficiency_score: 85,
            insights: vec![Insight {
                severity: Severity::Warning,
                rule: "test_rule".to_string(),
                message: "Test message".to_string(),
                suggested_fix: "Test fix".to_string(),
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        let deserialized: InsightsReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, deserialized);
    }

    // ── Boundary tests ───────────────────────────────────────────────────
    //
    // Every classification in this module uses a strict `>` comparison, so a
    // workload sitting *exactly* on a threshold belongs to the lower tier. The
    // tests below pin that behaviour down explicitly: the values are written
    // as literals rather than shared constants so that moving a threshold in
    // the implementation fails these tests instead of silently following along.

    /// Builds an insight of `severity` that contributes to the efficiency score
    /// without needing a rule to actually fire.
    fn synthetic(severity: Severity) -> Insight {
        Insight {
            severity,
            rule: "synthetic".to_string(),
            message: "synthetic".to_string(),
            suggested_fix: "synthetic".to_string(),
        }
    }

    /// Efficiency score with the insight deductions removed, isolating the
    /// graduated resource penalties.
    fn resource_only_score(r: &SorobanResources) -> u32 {
        InsightsEngine::compute_efficiency_score(r, &[])
    }

    // ── CPU tier boundaries (InstructionDensityRule) ──────────────────────

    #[test]
    fn test_instruction_density_cpu_exactly_at_warning_threshold_is_clean() {
        let r = SorobanResources {
            cpu_instructions: 10_000_000,
            ledger_read_bytes: 512,
            ledger_write_bytes: 256,
            ..Default::default()
        };
        assert!(InstructionDensityRule.evaluate(&r).is_empty());
    }

    #[test]
    fn test_instruction_density_cpu_one_above_warning_threshold_warns() {
        let r = SorobanResources {
            cpu_instructions: 10_000_001,
            ledger_read_bytes: 512,
            ledger_write_bytes: 256,
            ..Default::default()
        };
        let insights = InstructionDensityRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
    }

    #[test]
    fn test_instruction_density_cpu_exactly_at_critical_threshold_stays_warning() {
        let r = SorobanResources {
            cpu_instructions: 50_000_000,
            ledger_read_bytes: 512,
            ledger_write_bytes: 256,
            ..Default::default()
        };
        let insights = InstructionDensityRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
    }

    #[test]
    fn test_instruction_density_cpu_one_above_critical_threshold_is_critical() {
        let r = SorobanResources {
            cpu_instructions: 50_000_001,
            ledger_read_bytes: 512,
            ledger_write_bytes: 256,
            ..Default::default()
        };
        let insights = InstructionDensityRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Critical);
    }

    // ── Ledger gate boundaries (InstructionDensityRule) ───────────────────

    #[test]
    fn test_instruction_density_critical_gate_excludes_exactly_1024_ledger_bytes() {
        // CPU is far above the critical threshold, but total ledger I/O is
        // exactly 1_024 bytes so the `< 1_024` critical gate stays shut and
        // the workload drops to the warning tier (1_024 < 2_048).
        let r = SorobanResources {
            cpu_instructions: 80_000_000,
            ledger_read_bytes: 1_024,
            ..Default::default()
        };
        let insights = InstructionDensityRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
    }

    #[test]
    fn test_instruction_density_warning_gate_excludes_exactly_2048_ledger_bytes() {
        // Total ledger I/O is exactly 2_048 bytes, so neither gate opens even
        // though CPU is above the warning threshold.
        let r = SorobanResources {
            cpu_instructions: 15_000_000,
            ledger_read_bytes: 2_048,
            ..Default::default()
        };
        assert!(InstructionDensityRule.evaluate(&r).is_empty());
    }

    // ── RAM tier boundaries (MemoryPressureRule) ──────────────────────────

    #[test]
    fn test_memory_pressure_exactly_at_warning_threshold_is_clean() {
        let r = SorobanResources {
            ram_bytes: 5 * 1024 * 1024,
            ..Default::default()
        };
        assert!(MemoryPressureRule.evaluate(&r).is_empty());
    }

    #[test]
    fn test_memory_pressure_one_byte_above_warning_threshold_warns() {
        let r = SorobanResources {
            ram_bytes: 5 * 1024 * 1024 + 1,
            ..Default::default()
        };
        let insights = MemoryPressureRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
    }

    #[test]
    fn test_memory_pressure_exactly_at_critical_threshold_stays_warning() {
        let r = SorobanResources {
            ram_bytes: 20 * 1024 * 1024,
            ..Default::default()
        };
        let insights = MemoryPressureRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
    }

    #[test]
    fn test_memory_pressure_one_byte_above_critical_threshold_is_critical() {
        let r = SorobanResources {
            ram_bytes: 20 * 1024 * 1024 + 1,
            ..Default::default()
        };
        let insights = MemoryPressureRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Critical);
    }

    // ── Footprint key-count boundaries (FootprintBloatRule) ───────────────
    //
    // The estimated key count is `(reads + writes) / 60`, so the tier edges
    // land on exact multiples of 60 bytes: 600 → 10 keys, 1_200 → 20 keys.

    #[test]
    fn test_footprint_bloat_exactly_10_keys_is_clean() {
        let r = SorobanResources {
            ledger_read_bytes: 600,
            ..Default::default()
        };
        assert!(FootprintBloatRule.evaluate(&r).is_empty());
    }

    #[test]
    fn test_footprint_bloat_11_keys_warns() {
        let r = SorobanResources {
            ledger_read_bytes: 660,
            ..Default::default()
        };
        let insights = FootprintBloatRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
    }

    #[test]
    fn test_footprint_bloat_exactly_20_keys_stays_warning() {
        let r = SorobanResources {
            ledger_read_bytes: 1_200,
            ..Default::default()
        };
        let insights = FootprintBloatRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
    }

    #[test]
    fn test_footprint_bloat_21_keys_is_critical() {
        let r = SorobanResources {
            ledger_read_bytes: 1_260,
            ..Default::default()
        };
        let insights = FootprintBloatRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Critical);
    }

    #[test]
    fn test_footprint_bloat_key_estimate_uses_combined_read_and_write_bytes() {
        // 512 + 448 = 960 bytes → 16 estimated keys → warning tier. Neither
        // side on its own would reach the threshold.
        let r = SorobanResources {
            ledger_read_bytes: 512,
            ledger_write_bytes: 448,
            ..Default::default()
        };
        let insights = FootprintBloatRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
    }

    // ── Write-ratio boundaries (StorageEfficiencyRule) ────────────────────

    #[test]
    fn test_storage_efficiency_ratio_exactly_1_is_clean() {
        let r = SorobanResources {
            ledger_write_bytes: 1_024,
            transaction_size_bytes: 1_024,
            ..Default::default()
        };
        assert!(StorageEfficiencyRule.evaluate(&r).is_empty());
    }

    #[test]
    fn test_storage_efficiency_ratio_just_above_1_warns() {
        let r = SorobanResources {
            ledger_write_bytes: 1_025,
            transaction_size_bytes: 1_024,
            ..Default::default()
        };
        let insights = StorageEfficiencyRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
    }

    #[test]
    fn test_storage_efficiency_ratio_exactly_2_stays_warning() {
        let r = SorobanResources {
            ledger_write_bytes: 2_048,
            transaction_size_bytes: 1_024,
            ..Default::default()
        };
        let insights = StorageEfficiencyRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Warning);
    }

    #[test]
    fn test_storage_efficiency_ratio_just_above_2_is_critical() {
        let r = SorobanResources {
            ledger_write_bytes: 2_049,
            transaction_size_bytes: 1_024,
            ..Default::default()
        };
        let insights = StorageEfficiencyRule.evaluate(&r);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].severity, Severity::Critical);
    }

    #[test]
    fn test_storage_efficiency_zero_write_bytes_never_trips() {
        // Guards the degenerate 0/positive ratio.
        let r = SorobanResources {
            ledger_write_bytes: 0,
            transaction_size_bytes: 1,
            ..Default::default()
        };
        assert!(StorageEfficiencyRule.evaluate(&r).is_empty());
    }

    // ── Efficiency score arithmetic ───────────────────────────────────────

    #[test]
    fn test_score_deducts_20_per_critical_insight() {
        let r = SorobanResources::default();
        let insights = vec![synthetic(Severity::Critical)];
        assert_eq!(InsightsEngine::compute_efficiency_score(&r, &insights), 80);
    }

    #[test]
    fn test_score_deducts_10_per_warning_insight() {
        let r = SorobanResources::default();
        let insights = vec![synthetic(Severity::Warning)];
        assert_eq!(InsightsEngine::compute_efficiency_score(&r, &insights), 90);
    }

    #[test]
    fn test_score_deducts_3_per_info_insight() {
        let r = SorobanResources::default();
        let insights = vec![synthetic(Severity::Info)];
        assert_eq!(InsightsEngine::compute_efficiency_score(&r, &insights), 97);
    }

    #[test]
    fn test_score_clamps_to_zero_when_penalties_exceed_budget() {
        // 100 − (10 × 20) = −100 before the clamp.
        let r = SorobanResources::default();
        let insights: Vec<Insight> = (0..10).map(|_| synthetic(Severity::Critical)).collect();
        assert_eq!(InsightsEngine::compute_efficiency_score(&r, &insights), 0);
    }

    #[test]
    fn test_ledger_saturation_does_not_underflow_the_score() {
        // read + write saturates at u64::MAX rather than wrapping or panicking.
        // Footprint bloat and memory pressure both fire (2 × −20), and the
        // CPU / RAM / ledger penalties each take −10: 100 − 40 − 30 = 30.
        let r = SorobanResources {
            cpu_instructions: u64::MAX,
            ram_bytes: u64::MAX,
            ledger_read_bytes: u64::MAX,
            ledger_write_bytes: u64::MAX,
            transaction_size_bytes: 0,
        };
        let report = InsightsEngine::new().analyze(&r);
        assert_eq!(report.efficiency_score, 30);
    }

    // ── Graduated resource penalties (CPU / RAM / ledger) ─────────────────

    #[test]
    fn test_cpu_penalty_absent_at_10m_boundary() {
        let r = SorobanResources {
            cpu_instructions: 10_000_000,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 100);
    }

    #[test]
    fn test_cpu_penalty_is_5_points_above_10m_boundary() {
        let r = SorobanResources {
            cpu_instructions: 10_000_001,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 95);
    }

    #[test]
    fn test_cpu_penalty_stays_5_points_at_50m_boundary() {
        let r = SorobanResources {
            cpu_instructions: 50_000_000,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 95);
    }

    #[test]
    fn test_cpu_penalty_is_10_points_above_50m_boundary() {
        let r = SorobanResources {
            cpu_instructions: 50_000_001,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 90);
    }

    #[test]
    fn test_ram_penalty_absent_at_5mib_boundary() {
        let r = SorobanResources {
            ram_bytes: 5 * 1024 * 1024,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 100);
    }

    #[test]
    fn test_ram_penalty_is_5_points_above_5mib_boundary() {
        let r = SorobanResources {
            ram_bytes: 5 * 1024 * 1024 + 1,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 95);
    }

    #[test]
    fn test_ram_penalty_stays_5_points_at_20mib_boundary() {
        let r = SorobanResources {
            ram_bytes: 20 * 1024 * 1024,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 95);
    }

    #[test]
    fn test_ram_penalty_is_10_points_above_20mib_boundary() {
        let r = SorobanResources {
            ram_bytes: 20 * 1024 * 1024 + 1,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 90);
    }

    #[test]
    fn test_ledger_penalty_absent_at_50kib_boundary() {
        let r = SorobanResources {
            ledger_read_bytes: 50 * 1024,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 100);
    }

    #[test]
    fn test_ledger_penalty_is_5_points_above_50kib_boundary() {
        let r = SorobanResources {
            ledger_read_bytes: 50 * 1024 + 1,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 95);
    }

    #[test]
    fn test_ledger_penalty_stays_5_points_at_100kib_boundary() {
        let r = SorobanResources {
            ledger_read_bytes: 100 * 1024,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 95);
    }

    #[test]
    fn test_ledger_penalty_is_10_points_above_100kib_boundary() {
        let r = SorobanResources {
            ledger_read_bytes: 100 * 1024 + 1,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 90);
    }

    #[test]
    fn test_ledger_penalty_sums_read_and_write_bytes() {
        // Neither side alone crosses 50 KiB, but the combined total does.
        let r = SorobanResources {
            ledger_read_bytes: 30 * 1024,
            ledger_write_bytes: 25 * 1024,
            ..Default::default()
        };
        assert_eq!(resource_only_score(&r), 95);
    }

    // ── Engine integration ────────────────────────────────────────────────

    #[test]
    fn test_engine_evaluates_every_builtin_rule_in_registration_order() {
        // Sized so that all four rules fire exactly once: the write ratio is
        // 2.0 (warning), CPU is above 10M with 1_024 ledger bytes (warning),
        // 1_024 bytes estimates to 17 keys (warning) and RAM is 10 MiB
        // (warning).
        let r = SorobanResources {
            cpu_instructions: 15_000_000,
            ram_bytes: 10 * 1024 * 1024,
            ledger_read_bytes: 512,
            ledger_write_bytes: 512,
            transaction_size_bytes: 256,
        };
        let report = InsightsEngine::new().analyze(&r);
        let rules: Vec<&str> = report.insights.iter().map(|i| i.rule.as_str()).collect();
        assert_eq!(
            rules,
            vec![
                "storage_efficiency",
                "instruction_density",
                "footprint_bloat",
                "memory_pressure",
            ]
        );
        // 100 − (4 × 10 warning) − 5 (CPU) − 5 (RAM) = 50.
        assert_eq!(report.efficiency_score, 50);
    }

    #[test]
    fn test_engine_emits_at_most_one_insight_per_rule() {
        let r = SorobanResources {
            cpu_instructions: 80_000_000,
            ram_bytes: 30 * 1024 * 1024,
            ledger_read_bytes: 8_192,
            ledger_write_bytes: 8_192,
            transaction_size_bytes: 64,
        };
        let report = InsightsEngine::new().analyze(&r);
        let mut rules: Vec<&str> = report.insights.iter().map(|i| i.rule.as_str()).collect();
        let total = rules.len();
        rules.sort_unstable();
        rules.dedup();
        assert_eq!(rules.len(), total, "a rule produced more than one insight");
    }

    // ── Property-based tests: resource ratio calculations ────────────────
    //
    // The four ratios this module computes are:
    //   * `ledger_write_bytes / transaction_size_bytes` (storage efficiency)
    //   * `ledger_read_bytes + ledger_write_bytes`         (ledger pressure)
    //   * `(ledger_read_bytes + ledger_write_bytes) / 60`  (footprint keys)
    //   * `ledger_read_bytes + ledger_write_bytes`         (score penalties)
    // Each is exercised below over the full `u64` domain rather than a handful
    // of hand-picked values.

    use proptest::prelude::*;

    /// Ranks a storage-efficiency outcome so tiers can be compared as numbers:
    /// no insight (0) < Warning (1) < Critical (2).
    fn storage_severity_rank(r: &SorobanResources) -> u8 {
        match StorageEfficiencyRule
            .evaluate(r)
            .first()
            .map(|i| i.severity)
        {
            None => 0,
            Some(Severity::Warning) => 1,
            Some(Severity::Critical) => 2,
            // The storage rule never emits Info; rank it with "no insight" so
            // the ordering stays total.
            Some(Severity::Info) => 0,
        }
    }

    /// Same ordering for the footprint rule.
    fn footprint_severity_rank(r: &SorobanResources) -> u8 {
        match FootprintBloatRule.evaluate(r).first().map(|i| i.severity) {
            None => 0,
            Some(Severity::Warning) => 1,
            Some(Severity::Critical) => 2,
            Some(Severity::Info) => 0,
        }
    }

    fn storage_resources(writes: u64, tx_size: u64) -> SorobanResources {
        SorobanResources {
            ledger_write_bytes: writes,
            transaction_size_bytes: tx_size,
            ..Default::default()
        }
    }

    proptest! {
        /// Scaling write bytes and transaction size by the same factor must
        /// leave the write ratio — and therefore the tier — unchanged.
        #[test]
        fn prop_write_ratio_is_scale_invariant(
            writes in 0u64..10_000_000,
            tx_size in 1u64..1_000_000,
            scale in 1u64..10_000,
        ) {
            let base = writes as f64 / tx_size as f64;
            let scaled = (writes * scale) as f64 / (tx_size * scale) as f64;
            // Binary floating point is not required to divide exactly, so
            // compare with a relative tolerance.
            let tolerance = base.abs() * 1e-9 + f64::EPSILON;
            prop_assert!((base - scaled).abs() <= tolerance);

            prop_assert_eq!(
                storage_severity_rank(&storage_resources(writes, tx_size)),
                storage_severity_rank(&storage_resources(writes * scale, tx_size * scale)),
            );
        }

        /// Growing write bytes against a fixed transaction size can never
        /// downgrade the tier.
        #[test]
        fn prop_write_ratio_tier_is_monotonic_in_write_bytes(
            writes in 0u64..10_000_000,
            bump in 0u64..10_000_000,
            tx_size in 1u64..1_000_000,
        ) {
            let before = storage_severity_rank(&storage_resources(writes, tx_size));
            let after = storage_severity_rank(&storage_resources(writes + bump, tx_size));
            prop_assert!(after >= before);
        }

        /// The reported tier always agrees with the ratio band
        /// (≤ 1.0 clean, ≤ 2.0 warning, otherwise critical).
        #[test]
        fn prop_write_ratio_tier_matches_ratio_band(
            writes in 0u64..5_000_000,
            tx_size in 1u64..1_000_000,
        ) {
            let ratio = writes as f64 / tx_size as f64;
            match storage_severity_rank(&storage_resources(writes, tx_size)) {
                0 => prop_assert!(ratio <= 1.0),
                1 => prop_assert!(ratio > 1.0 && ratio <= 2.0),
                _ => prop_assert!(ratio > 2.0),
            }
        }

        /// A zero transaction size short-circuits before the division, so the
        /// rule must stay silent rather than reporting a NaN/inf comparison.
        #[test]
        fn prop_write_ratio_is_finite_over_the_full_u64_domain(
            writes in 0u64..=u64::MAX,
            tx_size in 0u64..=u64::MAX,
        ) {
            let r = storage_resources(writes, tx_size);
            let insights = StorageEfficiencyRule.evaluate(&r);
            prop_assert!(insights.len() <= 1);

            if tx_size == 0 {
                prop_assert!(insights.is_empty());
            } else {
                let ratio = writes as f64 / tx_size as f64;
                prop_assert!(ratio.is_finite());
                prop_assert!(ratio >= 0.0);
            }
        }

        /// Estimated key count is monotonic in total ledger bytes, and so is
        /// the resulting tier.
        #[test]
        fn prop_estimated_key_count_is_monotonic(
            reads in 0u64..1_000_000,
            writes in 0u64..1_000_000,
            bump in 0u64..100_000,
        ) {
            let before = SorobanResources {
                ledger_read_bytes: reads,
                ledger_write_bytes: writes,
                ..Default::default()
            };
            let after = SorobanResources {
                ledger_read_bytes: reads + bump,
                ledger_write_bytes: writes,
                ..Default::default()
            };

            let keys = |r: &SorobanResources| (r.ledger_read_bytes + r.ledger_write_bytes) / 60;
            prop_assert!(keys(&after) >= keys(&before));
            prop_assert!(footprint_severity_rank(&after) >= footprint_severity_rank(&before));
        }

        /// The graduated score penalties are monotonic: consuming more CPU,
        /// RAM or ledger I/O can never raise the resource-only score.
        #[test]
        fn prop_resource_penalties_are_monotonic(
            cpu in 0u64..200_000_000,
            ram in 0u64..64 * 1024 * 1024,
            reads in 0u64..256 * 1024,
            writes in 0u64..256 * 1024,
            bump in 0u64..50_000_000,
        ) {
            let before = SorobanResources {
                cpu_instructions: cpu,
                ram_bytes: ram,
                ledger_read_bytes: reads,
                ledger_write_bytes: writes,
                ..Default::default()
            };
            let after = SorobanResources {
                cpu_instructions: cpu + bump,
                ram_bytes: ram + bump,
                ledger_read_bytes: reads + bump,
                ledger_write_bytes: writes + bump,
                ..Default::default()
            };

            let before_score = resource_only_score(&before);
            let after_score = resource_only_score(&after);
            prop_assert!(after_score <= before_score);
        }

        /// The reported score is always inside the documented 0–100 range, and
        /// each built-in rule contributes at most one insight.
        #[test]
        fn prop_efficiency_score_is_bounded(
            cpu in 0u64..=u64::MAX,
            ram in 0u64..=u64::MAX,
            reads in 0u64..=u64::MAX,
            writes in 0u64..=u64::MAX,
            tx_size in 0u64..=u64::MAX,
        ) {
            let r = SorobanResources {
                cpu_instructions: cpu,
                ram_bytes: ram,
                ledger_read_bytes: reads,
                ledger_write_bytes: writes,
                transaction_size_bytes: tx_size,
            };
            let report = InsightsEngine::new().analyze(&r);
            prop_assert!(report.efficiency_score <= 100);
            prop_assert!(report.insights.len() <= 4);
        }
    }
}
