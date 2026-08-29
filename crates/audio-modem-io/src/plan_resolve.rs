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
