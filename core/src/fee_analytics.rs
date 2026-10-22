#![allow(clippy::unnecessary_cast)]

use crate::fee_store::LedgerFeeSample;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Direction of fee trend
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub enum TrendDirection {
    Upward,
    Downward,
    Stable,
}

/// Fee market prediction with multiple bidding strategies
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeePrediction {
    /// Current ledger sequence
    pub current_ledger: u64,
    /// Recommended bid for next ledger inclusion
    pub next_ledger_bid: u64,
    /// Recommended bid for inclusion within 3 ledgers
    pub next_3_ledgers_bid: u64,
    /// Economy bid (lowest cost, may take longer)
    pub economy_bid: u64,
    /// Standard bid (balanced)
    pub standard_bid: u64,
    /// Priority bid (fast inclusion)
    pub priority_bid: u64,
    /// Urgent bid (guaranteed next ledger)
    pub urgent_bid: u64,
    /// Confidence score (0.0-1.0)
    pub confidence_score: f64,
    /// Market volatility index
    pub market_volatility: f64,
    /// Trend direction
    pub trend_direction: TrendDirection,
}

/// Detailed model breakdown for transparency
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelBreakdown {
    /// 10-ledger Simple Moving Average
    pub sma_10: u64,
    /// 50-ledger Simple Moving Average
    pub sma_50: u64,
    /// 12-ledger Exponential Moving Average
    pub ema_12: u64,
    /// 50th percentile (median)
    pub percentile_50: u64,
    /// 75th percentile
    pub percentile_75: u64,
    /// 95th percentile
    pub percentile_95: u64,
    /// Standard deviation
    pub standard_deviation: f64,
    /// Coefficient of variation
    pub coefficient_of_variation: f64,
}

/// Market conditions snapshot
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MarketConditions {
    /// Current ledger sequence
    pub current_ledger: u64,
    /// Volatility classification
    pub volatility: String,
    /// Trend classification
    pub trend: String,
    /// Average fee over last 10 ledgers
    pub avg_fee_10_ledgers: u64,
    /// Average fee over last 50 ledgers
    pub avg_fee_50_ledgers: u64,
    /// Transaction pressure (0.0-1.0)
    pub transaction_pressure: f64,
}

/// Analytics engine that applies statistical models to fee data
pub struct FeeAnalyticsEngine {
    /// SMA window for short-term (default: 10)
    sma_short_window: usize,
    /// SMA window for medium-term (default: 50)
    sma_medium_window: usize,
    /// EMA smoothing factor (default: 12-ledger EMA)
    ema_period: usize,
    /// Safety margin multiplier (default: 1.10 = 10%)
    safety_margin: f64,
}

impl FeeAnalyticsEngine {
    /// Create a new analytics engine with default parameters
    pub fn new() -> Self {
        Self {
            sma_short_window: 10,
            sma_medium_window: 50,
            ema_period: 12,
            safety_margin: 1.10,
        }
    }

    /// Create with custom parameters
    pub fn with_params(
        sma_short_window: usize,
        sma_medium_window: usize,
        ema_period: usize,
        safety_margin: f64,
    ) -> Self {
        Self {
            sma_short_window,
            sma_medium_window,
            ema_period,
            safety_margin,
        }
    }

    /// Generate comprehensive fee prediction from historical samples
    pub fn predict(&self, samples: &[LedgerFeeSample], current_ledger: u64) -> FeePrediction {
        if samples.is_empty() {
            // Return defaults if no data available
            return FeePrediction {
                current_ledger,
                next_ledger_bid: 100,
                next_3_ledgers_bid: 100,
                economy_bid: 100,
                standard_bid: 100,
                priority_bid: 150,
                urgent_bid: 200,
                confidence_score: 0.0,
                market_volatility: 0.0,
                trend_direction: TrendDirection::Stable,
            };
        }

        // Extract base fees from samples (most recent first)
        let fees: Vec<i64> = samples.iter().map(|s| s.base_fee).collect();

        // Calculate statistical models
        let _sma_10 = self.calculate_sma(&fees, self.sma_short_window);
        let _sma_50 = self.calculate_sma(&fees, self.sma_medium_window);
        let _ema_12 = self.calculate_ema(&fees, self.ema_period);
        let p50 = self.calculate_percentile(&fees, 50.0);
        let p75 = self.calculate_percentile(&fees, 75.0);
        let p95 = self.calculate_percentile(&fees, 95.0);
        let std_dev = self.calculate_std_dev(&fees);
        let mean = self.calculate_mean(&fees);

        // Calculate coefficient of variation
        let cv = if mean > 0.0 { std_dev / mean } else { 0.0 };

        // Determine trend direction
        let trend = self.detect_trend(&fees);

        // Calculate volatility (coefficient of variation capped at 1.0)
        let volatility = cv.min(1.0);

        // Generate bid recommendations
        let economy_bid = (p50 as f64 * 0.9).ceil() as u64; // 90% of median
        let standard_bid = (p50 as f64 * self.safety_margin).ceil() as u64;
        let priority_bid = (p75 as f64 * self.safety_margin).ceil() as u64;
        let urgent_bid = (p95 as f64 * self.safety_margin).ceil() as u64;

        // Next ledger bid should be aggressive (use 95th percentile)
        let next_ledger_bid = urgent_bid;

        // Next 3 ledgers can be more conservative
        let next_3_ledgers_bid = standard_bid;

        // Confidence score based on data quantity and volatility
        let data_confidence = (samples.len() as f64 / 100.0).min(1.0);
        let volatility_confidence = 1.0 - volatility;
        let confidence_score = (data_confidence * 0.6 + volatility_confidence * 0.4).min(1.0);

        FeePrediction {
            current_ledger,
            next_ledger_bid,
            next_3_ledgers_bid,
            economy_bid,
            standard_bid,
            priority_bid,
            urgent_bid,
            confidence_score,
            market_volatility: volatility,
            trend_direction: trend,
        }
    }

    /// Get detailed model breakdown
    pub fn get_model_breakdown(&self, samples: &[LedgerFeeSample]) -> ModelBreakdown {
        if samples.is_empty() {
            return ModelBreakdown {
                sma_10: 100,
                sma_50: 100,
                ema_12: 100,
                percentile_50: 100,
                percentile_75: 100,
                percentile_95: 100,
                standard_deviation: 0.0,
                coefficient_of_variation: 0.0,
            };
        }

        let fees: Vec<i64> = samples.iter().map(|s| s.base_fee).collect();
        let sma_10 = self.calculate_sma(&fees, self.sma_short_window);
        let sma_50 = self.calculate_sma(&fees, self.sma_medium_window);
        let ema_12 = self.calculate_ema(&fees, self.ema_period);
        let p50 = self.calculate_percentile(&fees, 50.0);
        let p75 = self.calculate_percentile(&fees, 75.0);
        let p95 = self.calculate_percentile(&fees, 95.0);
        let std_dev = self.calculate_std_dev(&fees);
        let mean = self.calculate_mean(&fees);
        let cv = if mean > 0.0 { std_dev / mean } else { 0.0 };

        ModelBreakdown {
            sma_10,
            sma_50,
            ema_12,
            percentile_50: p50,
            percentile_75: p75,
            percentile_95: p95,
            standard_deviation: std_dev,
            coefficient_of_variation: cv,
        }
    }

    /// Get market conditions snapshot
    pub fn get_market_conditions(
        &self,
        samples: &[LedgerFeeSample],
        current_ledger: u64,
    ) -> MarketConditions {
        if samples.is_empty() {
            return MarketConditions {
                current_ledger,
                volatility: "unknown".to_string(),
                trend: "unknown".to_string(),
                avg_fee_10_ledgers: 100,
                avg_fee_50_ledgers: 100,
                transaction_pressure: 0.0,
            };
        }

        let fees: Vec<i64> = samples.iter().map(|s| s.base_fee).collect();
        let volatility = self.calculate_std_dev(&fees);
        let mean = self.calculate_mean(&fees);
        let cv = if mean > 0.0 { volatility / mean } else { 0.0 };

        let volatility_str = if cv < 0.1 {
            "low"
        } else if cv < 0.3 {
            "medium"
        } else {
            "high"
        }
        .to_string();

        let trend = self.detect_trend(&fees);
        let trend_str = match trend {
            TrendDirection::Upward => "upward",
            TrendDirection::Downward => "downward",
            TrendDirection::Stable => "stable",
        }
        .to_string();

        let avg_10 = self.calculate_sma(&fees, self.sma_short_window);
        let avg_50 = self.calculate_sma(&fees, self.sma_medium_window);

        // Calculate transaction pressure based on average tx count
        let avg_tx_count: f64 = samples
            .iter()
            .map(|s| s.transaction_count as f64)
            .sum::<f64>()
            / samples.len() as f64;

        // Normalize to 0-1 range (assuming max ~100 tx per ledger)
        let transaction_pressure = (avg_tx_count / 100.0).min(1.0);

        MarketConditions {
            current_ledger,
            volatility: volatility_str,
            trend: trend_str,
            avg_fee_10_ledgers: avg_10,
            avg_fee_50_ledgers: avg_50,
            transaction_pressure,
        }
    }

    // ── Statistical Methods ─────────────────────────────────────────────

    /// Calculate Simple Moving Average
    fn calculate_sma(&self, data: &[i64], window: usize) -> u64 {
        if data.is_empty() || window == 0 {
            return 0;
        }

        let window = window.min(data.len());
        let sum: i64 = data.iter().take(window).sum();
        (sum / window as i64).max(0) as u64
    }

    /// Calculate Exponential Moving Average
    fn calculate_ema(&self, data: &[i64], period: usize) -> u64 {
        if data.is_empty() || period == 0 {
            return 0;
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = data[0] as f64;

        for &value in data.iter().skip(1).take(period.min(data.len())) {
            ema = (value as f64 - ema) * multiplier + ema;
        }

        ema.max(0.0).round() as u64
    }

    /// Calculate percentile value
    fn calculate_percentile(&self, data: &[i64], percentile: f64) -> u64 {
        if data.is_empty() {
            return 0;
        }

        let mut sorted = data.to_vec();
        sorted.sort();

        let index = (percentile / 100.0) * (sorted.len() - 1) as f64;
        let lower = index.floor() as usize;
        let upper = index.ceil() as usize;
        let weight = index - lower as f64;

        if upper >= sorted.len() {
            sorted[lower].max(0) as u64
        } else {
            let value = sorted[lower] as f64 * (1.0 - weight) + sorted[upper] as f64 * weight;
            value.max(0.0).round() as u64
        }
    }

    /// Calculate standard deviation
    fn calculate_std_dev(&self, data: &[i64]) -> f64 {
        if data.len() < 2 {
            return 0.0;
        }

        let mean = self.calculate_mean(data);
        let variance: f64 = data
            .iter()
            .map(|&x| {
                let diff = x as f64 - mean;
                diff * diff
            })
            .sum::<f64>()
            / (data.len() - 1) as f64;

        variance.sqrt()
    }

    /// Calculate mean (average)
    fn calculate_mean(&self, data: &[i64]) -> f64 {
        if data.is_empty() {
            return 0.0;
        }
        data.iter().sum::<i64>() as f64 / data.len() as f64
    }

    /// Detect trend direction by comparing recent vs older averages
    fn detect_trend(&self, data: &[i64]) -> TrendDirection {
        if data.len() < 10 {
            return TrendDirection::Stable;
        }

        let recent_window = 5.min(data.len() / 2);
        let older_window = 5.min(data.len() / 2);

        let older: Vec<i64> = data.iter().take(older_window).cloned().collect();
        let recent: Vec<i64> = data
            .iter()
            .skip(data.len() - recent_window)
            .take(older_window)
            .cloned()
            .collect();

        let recent_avg = self.calculate_mean(&recent);
        let older_avg = self.calculate_mean(&older);

        // 5% threshold for trend detection
        let threshold = older_avg * 0.05;

        if recent_avg > older_avg + threshold {
            TrendDirection::Upward
        } else if recent_avg < older_avg - threshold {
            TrendDirection::Downward
        } else {
            TrendDirection::Stable
        }
    }
}

impl Default for FeeAnalyticsEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Historical fee chart data aggregator (#339) ───────────────────────────────
//
// Aggregates raw `LedgerFeeSample` records into chart-ready data points
// suitable for CLI output. Supports multiple resolution levels (per-ledger,
// or batched by 10/50/100 ledgers) and produces an ASCII chart for terminal
// display.

/// Resolution for fee chart data aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum FeeChartResolution {
    /// One data point per ledger (full granularity).
    Ledger,
    /// One data point per 10 ledgers (averaged).
    Batch10,
    /// One data point per 50 ledgers (averaged).
    Batch50,
    /// One data point per 100 ledgers (averaged).
    Batch100,
}

impl Default for FeeChartResolution {
    fn default() -> Self {
        FeeChartResolution::Ledger
    }
}

impl FeeChartResolution {
    /// Returns the number of ledgers per data point.
    fn batch_size(&self) -> usize {
        match self {
            FeeChartResolution::Ledger => 1,
            FeeChartResolution::Batch10 => 10,
            FeeChartResolution::Batch50 => 50,
            FeeChartResolution::Batch100 => 100,
        }
    }
}

/// A single data point in the fee chart.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeeChartDataPoint {
    /// Starting ledger sequence for this data point.
    pub ledger_sequence: i64,
    /// Average base fee across the aggregated ledgers.
    pub base_fee: i64,
    /// Average fee charged across the aggregated ledgers.
    pub fee_charged: i64,
    /// Maximum fee in the batch.
    pub max_fee: i64,
    /// Total transaction count across the aggregated ledgers.
    pub transaction_count: i64,
}

/// Summary statistics for the chart data.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeeChartSummary {
    /// Total number of ledgers represented.
    pub total_ledgers: usize,
    /// Minimum base fee observed.
    pub min_fee: i64,
    /// Maximum base fee observed.
    pub max_fee: i64,
    /// Average base fee across all ledgers.
    pub avg_fee: i64,
    /// Median base fee.
    pub median_fee: i64,
    /// Range (max - min).
    pub fee_range: i64,
    /// Total transactions across all ledgers.
    pub total_transactions: i64,
}

/// Aggregated fee chart data ready for CLI or API output.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeeChartData {
    /// The resolution used for aggregation.
    pub resolution: FeeChartResolution,
    /// Aggregated data points.
    pub data_points: Vec<FeeChartDataPoint>,
    /// Summary statistics.
    pub summary: FeeChartSummary,
}

/// Aggregator that converts raw `LedgerFeeSample` records into chart-ready data.
pub struct FeeChartAggregator;

impl FeeChartAggregator {
    /// Create a new aggregator instance.
    pub fn new() -> Self {
        FeeChartAggregator
    }

    /// Aggregate raw fee samples into chart data at the given resolution.
    ///
    /// Samples are sorted by ledger sequence before aggregation, ensuring
    /// deterministic output regardless of input order.
    pub fn aggregate(
        &self,
        samples: &[LedgerFeeSample],
        resolution: FeeChartResolution,
    ) -> FeeChartData {
        if samples.is_empty() {
            return FeeChartData {
                resolution,
                data_points: Vec::new(),
                summary: FeeChartSummary {
                    total_ledgers: 0,
                    min_fee: 0,
                    max_fee: 0,
                    avg_fee: 0,
                    median_fee: 0,
                    fee_range: 0,
                    total_transactions: 0,
                },
            };
        }

        // Sort by ledger sequence (ascending)
        let mut sorted: Vec<LedgerFeeSample> = samples.to_vec();
        sorted.sort_by_key(|s| s.ledger_sequence);

        // Compute summary statistics
        let all_fees: Vec<i64> = sorted.iter().map(|s| s.base_fee).collect();
        let total_tx: i64 = sorted.iter().map(|s| s.transaction_count as i64).sum();
        let min_fee = all_fees.iter().copied().min().unwrap_or(0);
        let max_fee = all_fees.iter().copied().max().unwrap_or(0);
        let avg_fee = all_fees.iter().sum::<i64>() / all_fees.len() as i64;
        let median_fee = Self::median(&all_fees);
        let fee_range = max_fee - min_fee;

        let summary = FeeChartSummary {
            total_ledgers: sorted.len(),
            min_fee,
            max_fee,
            avg_fee,
            median_fee,
            fee_range,
            total_transactions: total_tx,
        };

        // Aggregate data points at the requested resolution
        let batch_size = resolution.batch_size();
        let mut data_points = Vec::new();

        for chunk in sorted.chunks(batch_size) {
            if chunk.is_empty() {
                continue;
            }
            let first_ledger = chunk[0].ledger_sequence;
            let avg_base: i64 = chunk.iter().map(|s| s.base_fee).sum::<i64>() / chunk.len() as i64;
            let avg_charged: i64 =
                chunk.iter().map(|s| s.fee_charged).sum::<i64>() / chunk.len() as i64;
            let max_in_batch: i64 = chunk.iter().map(|s| s.max_fee).max().unwrap_or(0);
            let tx_count: i64 = chunk.iter().map(|s| s.transaction_count as i64).sum();

            data_points.push(FeeChartDataPoint {
                ledger_sequence: first_ledger,
                base_fee: avg_base,
                fee_charged: avg_charged,
                max_fee: max_in_batch,
                transaction_count: tx_count,
            });
        }

        FeeChartData {
            resolution,
            data_points,
            summary,
        }
    }

    /// Render fee chart data as an ASCII chart for CLI output.
    ///
    /// The chart displays a vertical bar representation of base fees, with
    /// each bar scaled to the terminal width. Includes a header with summary
    /// statistics.
    pub fn render_ascii(chart: &FeeChartData, max_width: usize) -> String {
        if chart.data_points.is_empty() {
            return "┌─ Fee Chart ───────────────────────────┐\n\
                    │ No data available                       │\n\
                    └────────────────────────────────────────┘\n"
                .to_string();
        }

        let max_width = max_width.max(20);
        let fees: Vec<i64> = chart.data_points.iter().map(|d| d.base_fee).collect();
        let min_val = fees.iter().copied().min().unwrap_or(0);
        let max_val = fees.iter().copied().max().unwrap_or(1);
        let range = (max_val - min_val).max(1) as f64;

        let mut out = String::new();
        out.push_str("┌─ FEE CHART ───────────────────────────────┐\n");
        out.push_str(&format!(
            "│ Ledgers: {}  Avg: {}  Min: {}  Max: {}  │\n",
            chart.summary.total_ledgers,
            chart.summary.avg_fee,
            chart.summary.min_fee,
            chart.summary.max_fee
        ));
        out.push_str("├───────────────────────────────────────────┤\n");

        // Determine how many data points to show (cap at max_width)
        let visible = chart.data_points.len().min(max_width);
        let step = if visible > 0 {
            chart.data_points.len() / visible
        } else {
            1
        };
        let step = step.max(1);

        for (i, dp) in chart.data_points.iter().enumerate().step_by(step) {
            let normalized = if range > 0.0 {
                ((dp.base_fee - min_val) as f64 / range * 30.0).round() as usize
            } else {
                15
            };
            let bar: String = "█".repeat(normalized.max(1));
            out.push_str(&format!(
                "│ L{} {:>6} │{}\n",
                dp.ledger_sequence, dp.base_fee, bar
            ));
            let _ = i;
        }

        out.push_str("└───────────────────────────────────────────┘\n");
        out
    }

    /// Calculate the median of a slice of i64 values.
    fn median(data: &[i64]) -> i64 {
        if data.is_empty() {
            return 0;
        }
        let mut sorted = data.to_vec();
        sorted.sort();
        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 0 {
            (sorted[mid - 1] + sorted[mid]) / 2
        } else {
            sorted[mid]
        }
    }
}

impl Default for FeeChartAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fee_store::LedgerFeeSample;
    use chrono::Utc;

    fn create_sample(ledger: i64, base_fee: i64) -> LedgerFeeSample {
        LedgerFeeSample {
            ledger_sequence: ledger,
            collected_at: Utc::now(),
            base_reserve: 0,
            base_fee,
            max_fee: base_fee,
            fee_charged: base_fee,
            transaction_count: 10,
            ledger_close_time: Utc::now(),
        }
    }

    #[test]
    fn test_sma_calculation() {
        let engine = FeeAnalyticsEngine::new();
        let data = vec![100, 110, 120, 130, 140];
        let sma = engine.calculate_sma(&data, 5);
        assert_eq!(sma, 120); // (100+110+120+130+140)/5 = 120
    }

    #[test]
    fn test_ema_calculation() {
        let engine = FeeAnalyticsEngine::new();
        let data = vec![100, 110, 120, 130, 140];
        let ema = engine.calculate_ema(&data, 5);
        // EMA should be higher than SMA (more weight on recent values)
        assert!(ema >= 120);
    }

    #[test]
    fn test_percentile_calculation() {
        let engine = FeeAnalyticsEngine::new();
        let data = vec![100, 200, 300, 400, 500];
        let p50 = engine.calculate_percentile(&data, 50.0);
        assert_eq!(p50, 300); // Median

        let p95 = engine.calculate_percentile(&data, 95.0);
        assert!(p95 >= 400);
    }

    #[test]
    fn test_std_dev_calculation() {
        let engine = FeeAnalyticsEngine::new();
        let data = vec![100, 100, 100, 100];
        let std_dev = engine.calculate_std_dev(&data);
        assert!(std_dev < 0.001); // Should be ~0 for identical values

        let data2 = vec![100, 200, 300, 400, 500];
        let std_dev2 = engine.calculate_std_dev(&data2);
        assert!(std_dev2 > 0.0); // Should be > 0 for varied values
    }

    #[test]
    fn test_trend_detection_upward() {
        let engine = FeeAnalyticsEngine::new();
        // Older values are lower, recent values are higher
        let data = vec![500, 400, 300, 200, 100, 150, 250, 350, 450, 550];
        let trend = engine.detect_trend(&data);
        assert_eq!(trend, TrendDirection::Upward);
    }

    #[test]
    fn test_trend_detection_downward() {
        let engine = FeeAnalyticsEngine::new();
        // Older values are higher, recent values are lower
        let data = vec![100, 200, 300, 400, 500, 450, 350, 250, 150, 50];
        let trend = engine.detect_trend(&data);
        assert_eq!(trend, TrendDirection::Downward);
    }

    #[test]
    fn test_trend_detection_stable() {
        let engine = FeeAnalyticsEngine::new();
        // All values similar
        let data = vec![100, 105, 102, 98, 103, 101, 99, 104, 100, 102];
        let trend = engine.detect_trend(&data);
        assert_eq!(trend, TrendDirection::Stable);
    }

    #[test]
    fn test_fee_prediction_with_data() {
        let engine = FeeAnalyticsEngine::new();
        let samples: Vec<LedgerFeeSample> = (0..50)
            .map(|i| create_sample(i as i64 + 1, 100 + (i % 10) * 5))
            .collect();

        let prediction = engine.predict(&samples, 100);

        assert_eq!(prediction.current_ledger, 100);
        assert!(prediction.economy_bid <= prediction.standard_bid);
        assert!(prediction.standard_bid <= prediction.priority_bid);
        assert!(prediction.priority_bid <= prediction.urgent_bid);
        assert!(prediction.confidence_score > 0.0);
    }

    #[test]
    fn test_fee_prediction_empty_data() {
        let engine = FeeAnalyticsEngine::new();
        let samples: Vec<LedgerFeeSample> = vec![];

        let prediction = engine.predict(&samples, 50);

        assert_eq!(prediction.current_ledger, 50);
        assert_eq!(prediction.next_ledger_bid, 100);
        assert_eq!(prediction.confidence_score, 0.0);
    }

    #[test]
    fn test_model_breakdown() {
        let engine = FeeAnalyticsEngine::new();
        let samples: Vec<LedgerFeeSample> = (0..20)
            .map(|i| create_sample(i as i64 + 1, 100 + i * 2))
            .collect();

        let breakdown = engine.get_model_breakdown(&samples);

        assert!(breakdown.sma_10 > 0);
        assert!(breakdown.sma_50 > 0);
        assert!(breakdown.ema_12 > 0);
        assert!(breakdown.percentile_50 > 0);
        assert!(breakdown.standard_deviation >= 0.0);
    }

    #[test]
    fn test_market_conditions() {
        let engine = FeeAnalyticsEngine::new();
        let samples: Vec<LedgerFeeSample> =
            (0..30).map(|i| create_sample(i as i64 + 1, 100)).collect();

        let conditions = engine.get_market_conditions(&samples, 100);

        assert_eq!(conditions.current_ledger, 100);
        assert_eq!(conditions.avg_fee_10_ledgers, 100);
        assert!(conditions.transaction_pressure >= 0.0);
        assert!(conditions.transaction_pressure <= 1.0);
    }

    // ── Historical fee chart data aggregator tests (#339) ──────────────────

    #[test]
    fn test_fee_chart_aggregator_empty_samples() {
        let aggregator = FeeChartAggregator::new();
        let chart = aggregator.aggregate(&[], FeeChartResolution::Ledger);
        assert_eq!(chart.data_points.len(), 0);
        assert_eq!(chart.summary.total_ledgers, 0);
    }

    #[test]
    fn test_fee_chart_aggregator_ledger_resolution() {
        let aggregator = FeeChartAggregator::new();
        let samples: Vec<LedgerFeeSample> = (0..10)
            .map(|i| create_sample(i as i64 + 1, 100 + i as i64 * 10))
            .collect();

        let chart = aggregator.aggregate(&samples, FeeChartResolution::Ledger);
        assert_eq!(chart.data_points.len(), 10);
        assert_eq!(chart.summary.total_ledgers, 10);
        assert_eq!(chart.data_points[0].base_fee, 100);
        assert_eq!(chart.data_points[1].base_fee, 110);
    }

    #[test]
    fn test_fee_chart_aggregator_batch_10_resolution() {
        let aggregator = FeeChartAggregator::new();
        let samples: Vec<LedgerFeeSample> =
            (0..30).map(|i| create_sample(i as i64 + 1, 100)).collect();

        let chart = aggregator.aggregate(&samples, FeeChartResolution::Batch10);
        // 30 ledgers / 10 per batch = 3 data points
        assert_eq!(chart.data_points.len(), 3);
        assert_eq!(chart.summary.total_ledgers, 30);
    }

    #[test]
    fn test_fee_chart_aggregator_batch_50_resolution() {
        let aggregator = FeeChartAggregator::new();
        let samples: Vec<LedgerFeeSample> =
            (0..100).map(|i| create_sample(i as i64 + 1, 100)).collect();

        let chart = aggregator.aggregate(&samples, FeeChartResolution::Batch50);
        // 100 ledgers / 50 per batch = 2 data points
        assert_eq!(chart.data_points.len(), 2);
    }

    #[test]
    fn test_fee_chart_aggregator_batch_100_resolution() {
        let aggregator = FeeChartAggregator::new();
        let samples: Vec<LedgerFeeSample> =
            (0..250).map(|i| create_sample(i as i64 + 1, 100)).collect();

        let chart = aggregator.aggregate(&samples, FeeChartResolution::Batch100);
        // 250 ledgers / 100 per batch = 3 data points (with remainder)
        assert_eq!(chart.data_points.len(), 3);
    }

    #[test]
    fn test_fee_chart_aggregator_summary_values() {
        let aggregator = FeeChartAggregator::new();
        let samples: Vec<LedgerFeeSample> = vec![
            create_sample(1, 100),
            create_sample(2, 200),
            create_sample(3, 300),
            create_sample(4, 400),
            create_sample(5, 500),
        ];

        let chart = aggregator.aggregate(&samples, FeeChartResolution::Ledger);
        assert_eq!(chart.summary.total_ledgers, 5);
        assert_eq!(chart.summary.min_fee, 100);
        assert_eq!(chart.summary.max_fee, 500);
        assert_eq!(chart.summary.avg_fee, 300);
        assert_eq!(chart.summary.median_fee, 300);
    }

    #[test]
    fn test_fee_chart_aggregator_fee_range() {
        let aggregator = FeeChartAggregator::new();
        let samples: Vec<LedgerFeeSample> = vec![create_sample(1, 100), create_sample(2, 500)];

        let chart = aggregator.aggregate(&samples, FeeChartResolution::Ledger);
        assert_eq!(chart.summary.fee_range, 400);
    }

    #[test]
    fn test_fee_chart_aggregator_tx_count() {
        let aggregator = FeeChartAggregator::new();
        let samples: Vec<LedgerFeeSample> =
            (0..5).map(|i| create_sample(i as i64 + 1, 100)).collect();

        let chart = aggregator.aggregate(&samples, FeeChartResolution::Ledger);
        assert_eq!(chart.summary.total_transactions, 25); // 5 ledgers * 5 tx each
    }

    #[test]
    fn test_fee_chart_aggregator_batch_averages() {
        let aggregator = FeeChartAggregator::new();
        let samples: Vec<LedgerFeeSample> = vec![
            create_sample(1, 100),
            create_sample(2, 200),
            create_sample(3, 300),
            create_sample(4, 400),
        ];

        // Batch10 should aggregate all 4 into one data point
        let chart = aggregator.aggregate(&samples, FeeChartResolution::Batch10);
        assert_eq!(chart.data_points.len(), 1);
        // Average of 100, 200, 300, 400 = 250
        assert_eq!(chart.data_points[0].base_fee, 250);
    }

    #[test]
    fn test_fee_chart_render_ascii_small() {
        let aggregator = FeeChartAggregator::new();
        let samples: Vec<LedgerFeeSample> = (0..10)
            .map(|i| create_sample(i as i64 + 1, 100 + i as i64 * 50))
            .collect();

        let chart = aggregator.aggregate(&samples, FeeChartResolution::Ledger);
        let ascii = FeeChartAggregator::render_ascii(&chart, 5);
        assert!(ascii.contains("FEE CHART"));
        assert!(ascii.contains("Base Fee"));
        assert!(ascii.contains("Ledger"));
        assert!(ascii.contains("100") || ascii.contains("550"));
    }

    #[test]
    fn test_fee_chart_render_ascii_empty() {
        let aggregator = FeeChartAggregator::new();
        let chart = aggregator.aggregate(&[], FeeChartResolution::Ledger);
        let ascii = FeeChartAggregator::render_ascii(&chart, 10);
        assert!(ascii.contains("FEE CHART") || ascii.contains("No data"));
    }

    #[test]
    fn test_fee_chart_aggregator_unsorted_input() {
        let aggregator = FeeChartAggregator::new();
        // Samples in reverse order
        let samples: Vec<LedgerFeeSample> = vec![
            create_sample(5, 500),
            create_sample(3, 300),
            create_sample(1, 100),
            create_sample(4, 400),
            create_sample(2, 200),
        ];

        let chart = aggregator.aggregate(&samples, FeeChartResolution::Ledger);
        // Should be sorted by ledger sequence
        assert_eq!(chart.data_points[0].ledger_sequence, 1);
        assert_eq!(chart.data_points[4].ledger_sequence, 5);
        assert_eq!(chart.data_points[0].base_fee, 100);
        assert_eq!(chart.data_points[4].base_fee, 500);
    }

    #[test]
    fn test_fee_chart_resolution_default() {
        assert_eq!(FeeChartResolution::default(), FeeChartResolution::Ledger);
    }
}
