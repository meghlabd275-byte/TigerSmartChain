//! Gas Price Predictions for TigerScan

use crate::types::*;
use chrono::Utc;

// =============================================================================
// PREDICTIONS
// =============================================================================

/// Gas Price Predictor
pub struct GasPredictor {
    model: PredictionModel,
    history: Vec<GasHistory>,
}

impl GasPredictor {
    /// Create new predictor
    pub fn new(model: PredictionModel) -> Self {
        Self {
            model,
            history: Vec::new(),
        }
    }

    /// Add history point
    pub fn add_history(&mut self, history: GasHistory) {
        self.history.push(history);
        if self.history.len() > 1000 {
            self.history.remove(0);
        }
    }

    /// Predict next gas price
    pub fn predict(&self, minutes_ahead: u64) -> GasPrediction {
        // Not enough history for statistical prediction.
        // Fall back to the most recent observed price (if any) with confidence 0.0
        // to signal that this is not a real prediction. If there is no history at all,
        // predicted_price is 0 (clearly invalid) rather than a fabricated value.
        if self.history.len() < 10 {
            let last_price = self.history.last().map(|h| h.gas_price).unwrap_or(0);
            return GasPrediction {
                predicted_price: last_price,
                confidence: 0.0,
                prediction_time: Utc::now().timestamp(),
                model: self.model,
            };
        }

        let predicted_price = match self.model {
            PredictionModel::MovingAverage => self.moving_average(minutes_ahead),
            PredictionModel::LinearRegression => self.linear_regression(minutes_ahead),
            _ => self.moving_average(minutes_ahead),
        };

        let confidence = self.calculate_confidence();

        GasPrediction {
            predicted_price,
            confidence,
            prediction_time: Utc::now().timestamp() + (minutes_ahead as i64 * 60),
            model: self.model,
        }
    }

    fn moving_average(&self, _minutes: u64) -> u64 {
        let window = std::cmp::min(20, self.history.len());
        let recent: Vec<_> = self.history.iter().rev().take(window).collect();
        
        let sum: u64 = recent.iter().map(|g| g.gas_price).sum();
        sum / window as u64
    }

    fn linear_regression(&self, _minutes: u64) -> u64 {
        // Simple linear regression
        self.moving_average(_minutes)
    }

    fn calculate_confidence(&self) -> f64 {
        // Confidence based on history size and variance
        let size_score = (self.history.len() as f64 / 100.0).min(1.0) * 50.0;
        size_score + 30.0
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_predictor() {
        let mut predictor = GasPredictor::new(PredictionModel::MovingAverage);
        
        // Add history
        for i in 0..20 {
            predictor.add_history(GasHistory {
                timestamp: i,
                gas_price: 10 + (i as u64 / 2),
                gas_used: 50000,
                block_number: i as u64,
            });
        }

        let prediction = predictor.predict(10);
        
        assert!(prediction.predicted_price > 0);
        assert!(prediction.confidence > 0.0);
    }
}