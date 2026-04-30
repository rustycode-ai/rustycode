//! Pricing information for LLM providers and models
//!
//! This module provides types for representing API costs in different currencies.

use serde::{Deserialize, Serialize};

/// Pricing information for a provider or model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingInfo {
    /// Cost per 1,000 input tokens
    pub input_cost_per_1k: f64,

    /// Cost per 1,000 output tokens
    pub output_cost_per_1k: f64,

    /// Currency for pricing
    pub currency: Currency,
}

impl PricingInfo {
    /// Calculate cost for a given number of tokens
    pub fn calculate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        let input_cost = (input_tokens as f64 / 1000.0) * self.input_cost_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * self.output_cost_per_1k;
        input_cost + output_cost
    }

    /// Get pricing in USD (converts if necessary)
    pub fn in_usd(&self) -> PricingInfo {
        PricingInfo {
            input_cost_per_1k: self.input_cost_per_1k * self.currency.usd_rate(),
            output_cost_per_1k: self.output_cost_per_1k * self.currency.usd_rate(),
            currency: Currency::Usd,
        }
    }
}

/// Currency types with exchange rates
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum Currency {
    /// US Dollar
    Usd,
    /// Euro
    Eur,
    /// British Pound
    Gbp,
    /// Japanese Yen
    Jpy,
    /// Canadian Dollar
    Cad,
    /// Australian Dollar
    Aud,
}

impl Currency {
    /// Get exchange rate to USD (as of 2025)
    ///
    /// Note: These rates should be updated periodically for accurate cost tracking
    pub fn usd_rate(self) -> f64 {
        match self {
            Currency::Usd => 1.0,
            Currency::Eur => 1.08,   // 1 EUR ≈ 1.08 USD
            Currency::Gbp => 1.27,   // 1 GBP ≈ 1.27 USD
            Currency::Jpy => 0.0067, // 1 JPY ≈ 0.0067 USD
            Currency::Cad => 0.74,   // 1 CAD ≈ 0.74 USD
            Currency::Aud => 0.65,   // 1 AUD ≈ 0.65 USD
        }
    }

    /// Get currency symbol
    pub fn symbol(self) -> &'static str {
        match self {
            Currency::Usd => "$",
            Currency::Eur => "€",
            Currency::Gbp => "£",
            Currency::Jpy => "¥",
            Currency::Cad => "C$",
            Currency::Aud => "A$",
        }
    }

    /// Get ISO 4217 currency code
    pub fn code(self) -> &'static str {
        match self {
            Currency::Usd => "USD",
            Currency::Eur => "EUR",
            Currency::Gbp => "GBP",
            Currency::Jpy => "JPY",
            Currency::Cad => "CAD",
            Currency::Aud => "AUD",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_cost() {
        let pricing = PricingInfo {
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
            currency: Currency::Usd,
        };

        // 1000 input + 500 output tokens
        let cost = pricing.calculate_cost(1000, 500);
        let expected = 0.003 + (0.5 * 0.015); // $0.003 + $0.0075 = $0.0105
        assert!((cost - expected).abs() < 0.0001);
    }

    #[test]
    fn test_eur_to_usd_conversion() {
        let pricing_eur = PricingInfo {
            input_cost_per_1k: 0.01, // €0.01 per 1k
            output_cost_per_1k: 0.02,
            currency: Currency::Eur,
        };

        let pricing_usd = pricing_eur.in_usd();
        assert_eq!(pricing_usd.currency, Currency::Usd);
        assert!((pricing_usd.input_cost_per_1k - 0.0108).abs() < 0.0001); // €0.01 ≈ $0.0108
    }

    #[test]
    fn test_currency_symbols() {
        assert_eq!(Currency::Usd.symbol(), "$");
        assert_eq!(Currency::Eur.symbol(), "€");
        assert_eq!(Currency::Gbp.symbol(), "£");
        assert_eq!(Currency::Jpy.symbol(), "¥");
    }

    #[test]
    fn test_currency_codes() {
        assert_eq!(Currency::Usd.code(), "USD");
        assert_eq!(Currency::Eur.code(), "EUR");
        assert_eq!(Currency::Gbp.code(), "GBP");
    }

    #[test]
    fn test_free_provider() {
        let pricing = PricingInfo {
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
            currency: Currency::Usd,
        };

        let cost = pricing.calculate_cost(10_000, 5_000);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_currency_usd_rate_is_one() {
        assert_eq!(Currency::Usd.usd_rate(), 1.0);
    }

    #[test]
    fn test_currency_all_rates_positive() {
        for currency in [
            Currency::Usd,
            Currency::Eur,
            Currency::Gbp,
            Currency::Jpy,
            Currency::Cad,
            Currency::Aud,
        ] {
            assert!(currency.usd_rate() > 0.0);
        }
    }

    #[test]
    fn test_currency_all_symbols_nonempty() {
        for currency in [
            Currency::Usd,
            Currency::Eur,
            Currency::Gbp,
            Currency::Jpy,
            Currency::Cad,
            Currency::Aud,
        ] {
            assert!(!currency.symbol().is_empty());
        }
    }

    #[test]
    fn test_currency_all_codes_three_letters() {
        for currency in [
            Currency::Usd,
            Currency::Eur,
            Currency::Gbp,
            Currency::Jpy,
            Currency::Cad,
            Currency::Aud,
        ] {
            assert_eq!(currency.code().len(), 3);
        }
    }

    #[test]
    fn test_currency_serde_roundtrip() {
        for currency in [
            Currency::Usd,
            Currency::Eur,
            Currency::Gbp,
            Currency::Jpy,
            Currency::Cad,
            Currency::Aud,
        ] {
            let json = serde_json::to_string(&currency).unwrap();
            let decoded: Currency = serde_json::from_str(&json).unwrap();
            assert_eq!(currency, decoded);
        }
    }

    #[test]
    fn test_currency_equality() {
        assert_eq!(Currency::Usd, Currency::Usd);
        assert_ne!(Currency::Usd, Currency::Eur);
    }

    #[test]
    fn test_pricing_info_roundtrip() {
        let pricing = PricingInfo {
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
            currency: Currency::Gbp,
        };
        let json = serde_json::to_string(&pricing).unwrap();
        let decoded: PricingInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.currency, Currency::Gbp);
        assert!((decoded.input_cost_per_1k - 0.003).abs() < 0.0001);
    }

    #[test]
    fn test_gbp_to_usd_conversion() {
        let pricing = PricingInfo {
            input_cost_per_1k: 1.0,
            output_cost_per_1k: 1.0,
            currency: Currency::Gbp,
        };
        let usd = pricing.in_usd();
        assert_eq!(usd.currency, Currency::Usd);
        assert!((usd.input_cost_per_1k - 1.27).abs() < 0.001);
    }

    #[test]
    fn test_jpy_to_usd_conversion() {
        let pricing = PricingInfo {
            input_cost_per_1k: 100.0,
            output_cost_per_1k: 200.0,
            currency: Currency::Jpy,
        };
        let usd = pricing.in_usd();
        assert_eq!(usd.currency, Currency::Usd);
        assert!(usd.input_cost_per_1k < 1.0); // JPY is much less than USD
    }

    #[test]
    fn test_calculate_cost_only_input() {
        let pricing = PricingInfo {
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
            currency: Currency::Usd,
        };
        let cost = pricing.calculate_cost(1000, 0);
        assert!((cost - 0.003).abs() < 0.0001);
    }

    #[test]
    fn test_calculate_cost_only_output() {
        let pricing = PricingInfo {
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
            currency: Currency::Usd,
        };
        let cost = pricing.calculate_cost(0, 2000);
        assert!((cost - 0.030).abs() < 0.0001);
    }
}
