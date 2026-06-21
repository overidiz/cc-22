use cc_22::Cc22;
use nih_plug::prelude::*;

fn main() {
    // The nih-plug standalone's WASAPI backend panics if the audio device
    // delivers a different period than the one it was configured with (default
    // 512). Detect the device's real shared-mode period up front and inject it
    // as a default, so the standalone "just works" on any machine without the
    // user having to pass `--period-size`.
    let mut args: Vec<String> = std::env::args().collect();
    inject_device_audio_defaults(&mut args);
    let _ = nih_export_standalone_with_args::<Cc22, _>(args);
}

fn inject_device_audio_defaults(args: &mut Vec<String>) {
    let has_period = args
        .iter()
        .any(|a| a == "--period-size" || a == "-p" || a.starts_with("--period-size="));
    if has_period {
        return;
    }

    #[cfg(target_os = "windows")]
    if let Some((sample_rate, period_size)) = detect_wasapi_config() {
        let has_sample_rate = args
            .iter()
            .any(|a| a == "--sample-rate" || a == "-r" || a.starts_with("--sample-rate="));
        if !has_sample_rate {
            args.push("--sample-rate".into());
            args.push(sample_rate.to_string());
        }
        args.push("--period-size".into());
        args.push(period_size.to_string());
    }

    #[cfg(not(target_os = "windows"))]
    let _ = args;
}

/// Query the default render device's shared-mode period (in samples) and sample
/// rate via WASAPI. Runs on a dedicated thread so its COM apartment never clashes
/// with the GUI's. Returns `None` if anything is unavailable.
#[cfg(target_os = "windows")]
fn detect_wasapi_config() -> Option<(u32, u32)> {
    std::thread::spawn(detect_wasapi_config_inner)
        .join()
        .ok()
        .flatten()
}

#[cfg(target_os = "windows")]
fn detect_wasapi_config_inner() -> Option<(u32, u32)> {
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_SHAREMODE_SHARED,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let result = (|| {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).ok()?;
            let mix = client.GetMixFormat().ok()?;
            if mix.is_null() {
                return None;
            }
            let sample_rate = (*mix).nSamplesPerSec;
            // The nominal "device period" (GetDevicePeriod) is NOT what WASAPI
            // shared mode actually delivers. Initialise a throwaway shared-mode
            // client and read GetBufferSize — that is the exact buffer size cpal
            // will receive, so matching `--period-size` to it avoids the assert.
            client
                .Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 0, 0, mix, None)
                .ok()?;
            let buffer_frames = client.GetBufferSize().ok()?;
            if sample_rate == 0 || buffer_frames == 0 {
                return None;
            }
            Some((sample_rate, buffer_frames.clamp(16, 8192)))
        })();
        CoUninitialize();
        result
    }
}
