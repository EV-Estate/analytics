//! Risk-premia / correlation model behind the overview terminal. Under
//! probabilistic-Kelly sizing with γ=1 the effective risk premia incurred is σ²/2
//! (the volatility drag), so compound growth g ≈ μ − σ²/2. The Vietnam sleeve is
//! accretive not through its own return but through its low correlation to the
//! alpha factors a host book already owns. See ../whitepaper/research/risk_premia.md.

pub struct Factor {
	pub label: &'static str,
	pub sigma: f64,
	pub rho: f64,
	pub default_exposure: f64,
}

pub struct Profile {
	pub factors: Vec<Factor>,
	pub mu_s: f64,
	pub sigma_s: f64,
}
impl Profile {
	/// Effect of swapping fraction `w` of the host book (factor exposures `w_k`, current
	/// return `mu_p`) into our instrument S. ponytail: factors treated as mutually
	/// uncorrelated for σ_P — indicative only, upgrade to a covariance matrix if a real
	/// profile is ever persisted.
	pub fn evaluate(&self, exposures: &[f64], mu_p: f64, w: f64) -> Outcome {
		assert_eq!(exposures.len(), self.factors.len(), "one exposure per factor");
		let var_p: f64 = self.factors.iter().zip(exposures).map(|(f, &wk)| (wk * f.sigma).powi(2)).sum();
		let sigma_p = var_p.sqrt();
		let cross: f64 = self.factors.iter().zip(exposures).map(|(f, &wk)| wk * f.rho * f.sigma).sum();
		// σ_P = 0 ⇒ no host risk ⇒ correlation undefined; 0 is the only sane display.
		let rho_sp = if sigma_p > 0.0 { cross / sigma_p } else { 0.0 };
		let mu_prime = (1.0 - w) * mu_p + w * self.mu_s;
		let var_prime = (1.0 - w).powi(2) * var_p + 2.0 * w * (1.0 - w) * rho_sp * self.sigma_s * sigma_p + w.powi(2) * self.sigma_s.powi(2);
		Outcome {
			delta_risk_premia: (var_p - var_prime) / 2.0,
			delta_performance: (mu_prime - var_prime / 2.0) - (mu_p - var_p / 2.0),
			rho_sp,
		}
	}
}

pub struct Outcome {
	pub delta_risk_premia: f64,
	pub delta_performance: f64,
	pub rho_sp: f64,
}

//dbg placeholder figures (whitepaper §IX / risk_premia ρ estimates, biased toward the
// Vietnam position) until the real estimated profile is measured and persisted.
pub fn profile() -> Profile {
	Profile {
		mu_s: 0.12,
		sigma_s: 0.18,
		factors: vec![
			Factor {
				label: "US Stocks (equity)",
				sigma: 0.16,
				rho: 0.18,
				default_exposure: 0.40,
			},
			Factor {
				label: "Bonds (duration)",
				sigma: 0.07,
				rho: -0.10,
				default_exposure: 0.25,
			},
			Factor {
				label: "Momentum / Trend",
				sigma: 0.12,
				rho: 0.05,
				default_exposure: 0.15,
			},
			Factor {
				label: "Carry",
				sigma: 0.10,
				rho: 0.12,
				default_exposure: 0.10,
			},
			Factor {
				label: "Mean-Reversion (value)",
				sigma: 0.11,
				rho: -0.05,
				default_exposure: 0.07,
			},
			Factor {
				label: "Gamma (convexity)",
				sigma: 0.20,
				rho: 0.08,
				default_exposure: 0.03,
			},
		],
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn uncorrelated_sleeve_sheds_risk_at_small_w() {
		let p = profile();
		let w_k: Vec<f64> = p.factors.iter().map(|f| f.default_exposure).collect();
		let at_zero = p.evaluate(&w_k, 0.10, 0.0);
		assert_eq!(at_zero.delta_risk_premia, 0.0, "w=0 changes nothing");
		assert_eq!(at_zero.delta_performance, 0.0, "w=0 changes nothing");
		// A small sleeve of a near-uncorrelated, higher-vol asset still nets risk shed —
		// the diversification term dominates its own variance contribution at low w.
		let small = p.evaluate(&w_k, 0.10, 0.10);
		assert!(small.delta_risk_premia > 0.0, "uncorrelated sleeve should shed risk at small w");
	}
}
