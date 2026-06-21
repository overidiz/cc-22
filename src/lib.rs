use nih_plug::prelude::*;
use std::sync::Arc;

pub mod dsp;
pub mod meters;
pub mod migration;
pub mod params;
pub mod presets;
pub mod ui;

#[cfg(test)]
mod mode_exposure_tests;

use dsp::denormals::FlushDenormals;
use dsp::Processor;
use meters::Meters;
use params::Cc22Params;

/// Reported processing tail in seconds. The diffusion/delay modes (Reels, Space,
/// Reverse, Collage, Cascade) keep ringing after input stops, so the host must be
/// told to keep the plugin alive long enough that those tails are never cut off.
/// Feedback is clamped below unity, so everything has decayed well within this.
const TAIL_SECONDS: f32 = 6.0;

pub struct Cc22 {
    params: Arc<Cc22Params>,
    meters: Arc<Meters>,
    processor: Processor,
    sample_rate: f32,
}

impl Default for Cc22 {
    fn default() -> Self {
        Self {
            params: Arc::new(Cc22Params::default()),
            meters: Arc::new(Meters::default()),
            processor: Processor::default(),
            sample_rate: 44_100.0,
        }
    }
}

impl Cc22 {
    /// Prepare the DSP layer for a new host sample rate.
    pub fn prepare(&mut self, sample_rate: f32) {
        self.processor.prepare(sample_rate);
    }

    /// Clear runtime DSP state without allocating.
    pub fn reset_dsp(&mut self) {
        self.params.reset_smoothers();
        self.processor.reset(self.params.global_bypass.value());
    }

    /// Process a host-provided block in place.
    pub fn process_block(&mut self, buffer: &mut Buffer) {
        self.processor
            .process_block(buffer, &self.params, &self.meters);
    }
}

impl Plugin for Cc22 {
    const NAME: &'static str = "CC-22";
    const VENDOR: &'static str = "Rafa Audio";
    const URL: &'static str = "https://github.com/overidiz/cc-22";
    const EMAIL: &'static str = "rafatoledoreis@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames::const_default(),
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    /// Migrate state saved by older CC-22 builds before it is applied, so that
    /// removed modes map onto the 20 official modes instead of silently falling
    /// back to defaults. See [`crate::migration`].
    fn filter_state(state: &mut PluginState) {
        crate::migration::migrate_legacy_state(state);
    }

    fn editor(&mut self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        ui::create_editor(self.params.clone(), self.meters.clone(), async_executor)
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.prepare(buffer_config.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.reset_dsp();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Flush denormals to zero for the duration of this block so the IIR /
        // feedback loops in the chain can't spike CPU on a decaying tail.
        let _denormals = FlushDenormals::new();
        self.process_block(buffer);
        ProcessStatus::Tail((self.sample_rate * TAIL_SECONDS) as u32)
    }
}

impl ClapPlugin for Cc22 {
    const CLAP_ID: &'static str = "com.rafaaudio.cc22";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A modular boutique-style multi-FX base with smoothed global controls.");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("mailto:rafatoledoreis@gmail.com");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Mono,
        ClapFeature::Stereo,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for Cc22 {
    const VST3_CLASS_ID: [u8; 16] = *b"CC22RafaAudio001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_clap!(Cc22);
nih_export_vst3!(Cc22);
