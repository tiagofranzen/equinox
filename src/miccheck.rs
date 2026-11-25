//! Proof that the EQ reaches a microphone. Windows lets an app open a capture
//! device "raw", which skips the endpoint's effects. Recording the raw and the
//! normal stream at the same moment and comparing them band by band shows the
//! EQ that is really being applied, whatever sound is in the room.

use std::time::{Duration, Instant};

use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMOPTIONS_RAW, AudioCategory_Other, AudioClientProperties,
    IAudioCaptureClient, IAudioClient, IAudioClient2, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::core::{HSTRING, Interface};

use crate::wasapi::Format;

const N: usize = 4096;
/// Octave bands compared, by centre frequency.
pub const BANDS: [f32; 8] = [63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];

#[derive(Clone, Debug)]
pub struct Check {
    /// Per band: centre Hz, measured change in dB (normal minus raw), expected change in dB.
    pub bands: Vec<(f32, f32, f32)>,
    /// Average measured and expected change over the bands.
    pub measured: f32,
    pub expected: f32,
    /// How far the measured changes are from the expected ones: the median
    /// over the bands, in dB, so one noisy band doesn't decide it.
    pub error: f32,
}

/// The measurement has to follow the EQ this closely to count as applied.
/// A working EQ lands within about 1 dB; another effect running in its place
/// (Windows Voice Clarity filters and gates the signal) is off by far more.
const TOLERANCE_DB: f32 = 3.0;

impl Check {
    /// The microphone's signal follows the EQ. `None` when the EQ is too
    /// close to flat to tell it from no EQ at all.
    pub fn applied(&self) -> Option<bool> {
        let size = (self.bands.iter().map(|b| b.2 * b.2).sum::<f32>() / self.bands.len().max(1) as f32).sqrt();
        (size >= 1.5).then_some(self.error <= TOLERANCE_DB)
    }
}

/// [`run`] against what the saved state asks Equalizer APO to do to `mic` right now.
pub fn run_for(state: &crate::model::State, mic: &str, seconds: f32) -> Result<Check, String> {
    let target = crate::model::Target::Mic(mic.to_string());
    let r = crate::engine::resolve(state, state.chain(&target));
    let (bypass, keep) = (state.mic_bypass, state.settings.level_matched_bypass);
    let (bands, graphic, preamp) = (r.all(), r.graphic.clone(), r.preamp);
    run(mic, seconds, move |f| match (bypass, keep) {
        (true, true) => preamp,
        (true, false) => 0.0,
        _ => crate::dsp::response(&bands, &graphic, &[f])[0] + preamp,
    })
}

/// Record `mic` raw and normally for `seconds` and compare with `expected`,
/// which gives the EQ's gain in dB (preamp included) at a frequency.
pub fn run(mic: &str, seconds: f32, expected: impl Fn(f32) -> f32) -> Result<Check, String> {
    let dur = Duration::from_secs_f32(seconds.clamp(0.5, 20.0));
    let (a, b) = (mic.to_string(), mic.to_string());
    let raw = std::thread::spawn(move || record(&a, true, dur));
    let normal = std::thread::spawn(move || record(&b, false, dur));
    let raw = raw.join().map_err(|_| "capture thread failed".to_string())??;
    let normal = normal.join().map_err(|_| "capture thread failed".to_string())??;
    let mut bands = Vec::new();
    for (i, &f) in BANDS.iter().enumerate() {
        if raw[i] <= 0.0 || normal[i] <= 0.0 {
            continue;
        }
        bands.push((f, (10.0 * (normal[i] / raw[i]).log10()) as f32, expected(f)));
    }
    if bands.is_empty() {
        return Err("the microphone delivered only silence (is it muted?)".into());
    }
    let n = bands.len() as f32;
    let mut off: Vec<f32> = bands.iter().map(|b| (b.1 - b.2).abs()).collect();
    off.sort_by(f32::total_cmp);
    Ok(Check {
        measured: bands.iter().map(|b| b.1).sum::<f32>() / n,
        expected: bands.iter().map(|b| b.2).sum::<f32>() / n,
        error: off[off.len() / 2],
        bands,
    })
}

/// Average power per band over the recording.
fn record(mic: &str, raw: bool, dur: Duration) -> Result<[f64; BANDS.len()], String> {
    crate::sys::com_init();
    unsafe { capture(mic, raw, dur) }.map_err(|e| {
        if raw {
            format!(
                "this microphone can't be opened raw, so there's nothing to compare against ({})",
                e.message()
            )
        } else {
            format!("couldn't open the microphone ({})", e.message())
        }
    })
}

unsafe fn capture(mic: &str, raw: bool, dur: Duration) -> windows::core::Result<[f64; BANDS.len()]> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = en.GetDevice(&HSTRING::from(crate::sys::endpoint_id(mic, true)))?;
        let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
        if raw {
            let props = AudioClientProperties {
                cbSize: std::mem::size_of::<AudioClientProperties>() as u32,
                bIsOffload: false.into(),
                eCategory: AudioCategory_Other,
                Options: AUDCLNT_STREAMOPTIONS_RAW,
            };
            client.cast::<IAudioClient2>()?.SetClientProperties(&props)?;
        }
        let fmt = client.GetMixFormat()?;
        let format = Format::read(fmt);
        let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 2_000_000, 0, fmt, None);
        CoTaskMemFree(Some(fmt as _));
        init?;
        let cap: IAudioCaptureClient = client.GetService()?;
        client.Start()?;

        let window: Vec<f32> = (0..N)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N as f32).cos())
            .collect();
        let mut power = vec![0f64; N / 2];
        let (mut block, mut all) = (Vec::with_capacity(N), Vec::new());
        let (mut re, mut im) = (vec![0f32; N], vec![0f32; N]);
        let start = Instant::now();
        while start.elapsed() < dur {
            let mut n = cap.GetNextPacketSize()?;
            while n > 0 {
                let mut data = std::ptr::null_mut();
                let (mut frames, mut flags) = (0u32, 0u32);
                cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                all.clear();
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 == 0 && !data.is_null() && format.channels > 0 {
                    format.floats(data, frames as usize, &mut all);
                }
                cap.ReleaseBuffer(frames)?;
                for f in all.chunks_exact(format.channels.max(1)) {
                    block.push(f.iter().sum::<f32>() / f.len() as f32);
                    if block.len() == N {
                        for i in 0..N {
                            re[i] = block[i] * window[i];
                            im[i] = 0.0;
                        }
                        crate::analyzer::fft(&mut re, &mut im);
                        for k in 0..N / 2 {
                            power[k] += (re[k] * re[k] + im[k] * im[k]) as f64;
                        }
                        block.clear();
                    }
                }
                n = cap.GetNextPacketSize()?;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = client.Stop();

        let bin = format.rate as f32 / N as f32;
        let mut out = [0f64; BANDS.len()];
        for (o, &f) in out.iter_mut().zip(&BANDS) {
            let (lo, hi) = ((f / 2f32.sqrt() / bin) as usize, (f * 2f32.sqrt() / bin) as usize);
            *o = power[lo.max(1)..hi.min(N / 2 - 1).max(lo.max(1) + 1)].iter().sum();
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(bands: &[(f32, f32, f32)]) -> Check {
        let mut off: Vec<f32> = bands.iter().map(|b| (b.1 - b.2).abs()).collect();
        off.sort_by(f32::total_cmp);
        Check {
            bands: bands.to_vec(),
            measured: 0.0,
            expected: 0.0,
            error: off[off.len() / 2],
        }
    }

    #[test]
    fn verdicts() {
        // Measured on a HyperX SoloCast with a +12 dB bass shelf and -14 dB preamp.
        let working = [
            (63.0, 0.0, -1.6),
            (250.0, -9.8, -10.2),
            (1000.0, -13.7, -13.7),
            (4000.0, -14.7, -14.5),
            (8000.0, -16.4, -17.4),
        ];
        assert_eq!(check(&working).applied(), Some(true));
        // Same EQ with Windows Voice Clarity running instead: its noise gate
        // can land near the preamp by accident in a band or two, never across them.
        let replaced = [
            (63.0, -36.0, -1.6),
            (250.0, -19.5, -10.2),
            (1000.0, -11.2, -13.7),
            (4000.0, -7.4, -14.5),
            (8000.0, -4.3, -17.4),
        ];
        assert_eq!(check(&replaced).applied(), Some(false));
        let untouched = [
            (63.0, 0.1, -1.6),
            (250.0, -0.2, -10.2),
            (1000.0, 0.0, -13.7),
            (4000.0, 0.1, -14.5),
            (8000.0, 0.0, -17.4),
        ];
        assert_eq!(check(&untouched).applied(), Some(false));
        // A flat EQ can't be told from no EQ.
        let flat = [(63.0, 0.0, 0.0), (1000.0, 0.1, 0.0), (8000.0, -0.1, 0.0)];
        assert_eq!(check(&flat).applied(), None);
    }
}
