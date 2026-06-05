pub mod bypass;
pub mod chain;
pub mod character;
pub mod diffusion;
pub mod dry_wet;
pub mod eq;
pub mod gain;
pub mod movement;
pub mod smoothing;
pub mod texture;

#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod validation_tests;

use nih_plug::prelude::Buffer;

use crate::{meters::Meters, params::Cc22Params};
use bypass::BypassCrossfade;
use chain::{sanitize_sample, EffectChain};
use dry_wet::DryWet;
use gain::GainStage;

pub struct Processor {
    input_gain: GainStage,
    chain: EffectChain,
    output_gain: GainStage,
    dry_wet: DryWet,
    bypass: BypassCrossfade,
    sample_rate: f32,
}

impl Default for Processor {
    fn default() -> Self {
        let mut processor = Self {
            input_gain: GainStage,
            chain: EffectChain::default(),
            output_gain: GainStage,
            dry_wet: DryWet,
            bypass: BypassCrossfade::default(),
            sample_rate: 44_100.0,
        };
        processor.prepare(44_100.0);
        processor
    }
}

impl Processor {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.chain.prepare(self.sample_rate);
        self.bypass.prepare(self.sample_rate);
    }

    pub fn reset(&mut self, bypassed: bool) {
        self.chain.reset();
        self.bypass.reset(bypassed);
    }

    pub fn process_block(&mut self, buffer: &mut Buffer, params: &Cc22Params, meters: &Meters) {
        self.bypass.set_bypassed(params.global_bypass.value());
        let mut input_peak = 0.0_f32;
        let mut output_peak = 0.0_f32;

        for channel_samples in buffer.iter_samples() {
            let input_gain = self
                .input_gain
                .db_to_gain(params.input_gain.smoothed.next());
            let output_gain = self
                .output_gain
                .db_to_gain(params.output_gain.smoothed.next());
            let dry_wet = params.dry_wet.smoothed.next();
            let active_mix = self.bypass.next_active_mix();
            let chain_frame = self.chain.next_frame(params);

            for (channel_index, sample) in channel_samples.into_iter().enumerate() {
                let dry_sample = *sample;
                input_peak = input_peak.max(dry_sample.abs());
                let wet_sample = self.input_gain.apply(dry_sample, input_gain);
                let wet_sample = self
                    .chain
                    .process_sample(channel_index, wet_sample, &chain_frame);
                let wet_sample = sanitize_sample(self.output_gain.apply(wet_sample, output_gain));
                let processed_sample = self.dry_wet.mix(dry_sample, wet_sample, dry_wet);

                *sample =
                    sanitize_sample(self.bypass.mix(dry_sample, processed_sample, active_mix));
                output_peak = output_peak.max(sample.abs());
            }
        }

        meters.publish_block(input_peak, output_peak);
    }
}
