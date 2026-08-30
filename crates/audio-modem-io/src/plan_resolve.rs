//! Resolve a [`Plan`] from a profile, a carrier's recorded metadata, and
//! per-field overrides — independent of how those overrides were collected.
//!
//! The CLI collects them as clap flags; the GUI collects them from a form.
//! Both need exactly the same resolution order (explicit override > profile >
//! carrier metadata > built-in default) and the same cross-field validation
//! (an OFDM-only override on an FSK plan is a mistake, not something to
//! silently ignore), so that logic lives here once.

use audio_modem_core::{Plan, Profile};

/// Every tone-plan parameter a caller might override.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlanOverrides {
    pub profile: Option<Profile>,
    pub sample_rate: Option<u32>,
    pub amplitude: Option<f32>,
    pub samples_per_symbol: Option<usize>,
    pub bits_per_symbol: Option<u32>,
    pub bin_spacing: Option<usize>,
    pub fft_size: Option<usize>,
    pub qam_bits: Option<u32>,
    pub top_bin: Option<usize>,
    pub base_bin: Option<usize>,
}

impl PlanOverrides {
    /// Whether the caller named any plan option at all.
    pub fn is_explicit(&self) -> bool {
        self.profile.is_some()
            || self.sample_rate.is_some()
            || self.amplitude.is_some()
            || self.base_bin.is_some()
            || self.fsk_flags_used()
            || self.ofdm_flags_used()
    }

    fn fsk_flags_used(&self) -> bool {
        self.samples_per_symbol.is_some()
            || self.bits_per_symbol.is_some()
            || self.bin_spacing.is_some()
    }

    fn ofdm_flags_used(&self) -> bool {
        self.fft_size.is_some() || self.qam_bits.is_some() || self.top_bin.is_some()
    }

    /// Resolve a validated plan.
    ///
    /// `recorded` is the plan read from a carrier's metadata, if any. It is
    /// the base when no profile was named, so individual overrides still
    /// apply on top of what the file says.
    pub fn resolve(&self, recorded: Option<Plan>) -> Result<Plan, String> {
        let mut plan = match (self.profile, recorded) {
            (Some(profile), _) => profile.plan(),
            (None, Some(recorded)) => recorded,
            (None, None) => Profile::Dense.plan(),
        };

        // Reject overrides that do not apply to the selected waveform rather
        // than silently ignoring them; a caller that names an OFDM-only field
        // for an FSK plan has a wrong mental model and should hear about it.
        match &mut plan {
            Plan::Fsk(config) => {
                if self.ofdm_flags_used() {
                    return Err(format!(
                        "fft_size, qam_bits and top_bin apply to OFDM plans; this plan is {}. \
                         Select the dense or compact profile to use OFDM.",
                        Plan::Fsk(*config).describe()
                    ));
                }
                if let Some(value) = self.samples_per_symbol {
                    config.samples_per_symbol = value;
                }
                if let Some(value) = self.bits_per_symbol {
                    config.bits_per_symbol = value;
                }
                if let Some(value) = self.bin_spacing {
                    config.bin_spacing = value;
                }
                if let Some(value) = self.base_bin {
                    config.base_bin = value;
                }
            }
            Plan::Ofdm(config) => {
                if self.fsk_flags_used() {
                    return Err(format!(
                        "samples_per_symbol, bits_per_symbol and bin_spacing apply to FSK \
                         plans; this plan is {}. Select the standard or fast profile to use FSK.",
                        Plan::Ofdm(*config).describe()
                    ));
                }
                if let Some(value) = self.fft_size {
                    config.fft_size = value;
                }
                if let Some(value) = self.qam_bits {
                    config.bits_per_bin = value;
                }
                if let Some(value) = self.top_bin {
                    config.top_bin = value;
                }
                if let Some(value) = self.base_bin {
                    config.base_bin = value;
                }
            }
        }

        if let Some(value) = self.sample_rate {
            plan.set_sample_rate(value);
        }
        if let Some(value) = self.amplitude {
            plan.set_amplitude(value);
        }

        plan.validate()
            .map_err(|error| format!("invalid plan: {error}"))?;

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_overrides_is_not_explicit() {
        assert!(!PlanOverrides::default().is_explicit());
    }

    #[test]
    fn any_single_field_makes_it_explicit() {
        assert!(PlanOverrides {
            profile: Some(Profile::Fast),
            ..Default::default()
        }
        .is_explicit());
        assert!(PlanOverrides {
            amplitude: Some(0.5),
            ..Default::default()
        }
        .is_explicit());
        assert!(PlanOverrides {
            qam_bits: Some(12),
            ..Default::default()
        }
        .is_explicit());
    }

    #[test]
    fn no_overrides_and_no_recorded_plan_falls_back_to_dense() {
        let plan = PlanOverrides::default().resolve(None).unwrap();
        assert_eq!(plan, Profile::Dense.plan());
    }

    #[test]
    fn an_explicit_profile_overrides_a_recorded_plan() {
        let recorded = Some(Profile::Compact.plan());
        let overrides = PlanOverrides {
            profile: Some(Profile::Fast),
            ..Default::default()
        };
        assert_eq!(overrides.resolve(recorded).unwrap(), Profile::Fast.plan());
    }

    #[test]
    fn no_profile_uses_the_recorded_plan_as_the_base() {
        let recorded = Profile::Standard.plan();
        let plan = PlanOverrides::default().resolve(Some(recorded)).unwrap();
        assert_eq!(plan, recorded);
    }

    #[test]
    fn an_ofdm_only_override_is_rejected_on_an_fsk_plan() {
        let overrides = PlanOverrides {
            profile: Some(Profile::Standard),
            qam_bits: Some(12),
            ..Default::default()
        };
        let error = overrides.resolve(None).unwrap_err();
        assert!(error.contains("OFDM"), "got: {error}");
    }

    #[test]
    fn an_fsk_only_override_is_rejected_on_an_ofdm_plan() {
        let overrides = PlanOverrides {
            profile: Some(Profile::Dense),
            bits_per_symbol: Some(4),
            ..Default::default()
        };
        let error = overrides.resolve(None).unwrap_err();
        assert!(error.contains("FSK"), "got: {error}");
    }

    #[test]
    fn sample_rate_and_amplitude_apply_to_either_waveform() {
        let ofdm = PlanOverrides {
            profile: Some(Profile::Dense),
            sample_rate: Some(48_000),
            amplitude: Some(0.5),
            ..Default::default()
        }
        .resolve(None)
        .unwrap();
        assert_eq!(ofdm.sample_rate(), 48_000);
        assert_eq!(ofdm.amplitude(), 0.5);

        let fsk = PlanOverrides {
            profile: Some(Profile::Standard),
            sample_rate: Some(48_000),
            amplitude: Some(0.5),
            ..Default::default()
        }
        .resolve(None)
        .unwrap();
        assert_eq!(fsk.sample_rate(), 48_000);
        assert_eq!(fsk.amplitude(), 0.5);
    }

    #[test]
    fn base_bin_above_top_bin_is_rejected_by_validation() {
        let overrides = PlanOverrides {
            profile: Some(Profile::Dense),
            base_bin: Some(500),
            top_bin: Some(10),
            ..Default::default()
        };
        let error = overrides.resolve(None).unwrap_err();
        assert!(error.starts_with("invalid plan:"), "got: {error}");
    }
}
