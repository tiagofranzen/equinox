//! "Hear yourself": plays a microphone through the default output while you
//! tune it. The microphone is opened in shared mode, which is after its effects,
//! so what you hear is the microphone with equinox's EQ applied.
//!
//! Capture is event driven and the playback queue is kept short, so the delay
//! stays around a few hundredths of a second.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioCaptureClient, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator,
    MMDeviceEnumerator, WAVEFORMATEX, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::HSTRING;

use crate::wasapi::Format;

/// Playback is always stereo float; Windows converts the rate for the output.
const OUT_CHANNELS: usize = 2;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

struct Shared {
    stop: AtomicBool,
    /// Playback volume, f32 bits (1.0 = as loud as the microphone delivers).
    volume: AtomicU32,
    /// Loudest sample of the last packets, f32 bits, for a level meter.
    peak: AtomicU32,
    /// Frames passed along so far (0 until sound flows).
    frames: AtomicU32,
    error: Mutex<Option<String>>,
}

pub struct Monitor {
    mic: String,
    shared: Arc<Shared>,
}

impl Monitor {
    /// Start playing the microphone with this endpoint GUID through the default output.
    pub fn start(mic: &str, volume: f32) -> Monitor {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            volume: AtomicU32::new(volume.to_bits()),
            peak: AtomicU32::new(0f32.to_bits()),
            frames: AtomicU32::new(0),
            error: Mutex::new(None),
        });
        let (s, guid) = (shared.clone(), mic.to_string());
        std::thread::Builder::new()
            .name("monitor".into())
            .spawn(move || {
                crate::sys::com_init();
                while !s.stop.load(Ordering::Relaxed) {
                    // Re-opens when the default output changes and after errors (unplugged...).
                    let r = unsafe { session(&guid, &s) };
                    if let Ok(mut e) = s.error.lock() {
                        *e = r.err().map(|e| explain(&e));
                    }
                    if !s.stop.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
            })
            .ok();
        Monitor {
            mic: mic.to_string(),
            shared,
        }
    }

    /// The microphone being played back.
    pub fn mic(&self) -> &str {
        &self.mic
    }

    pub fn set_volume(&self, volume: f32) {
        self.shared.volume.store(volume.clamp(0.0, 4.0).to_bits(), Ordering::Relaxed);
    }

    /// Why nothing is playing, if something went wrong.
    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().ok().and_then(|e| e.clone())
    }

    /// Whether sound has started flowing.
    pub fn running(&self) -> bool {
        self.shared.frames.load(Ordering::Relaxed) > 0
    }

    /// Frames passed from the microphone to the output so far.
    pub fn frames(&self) -> u32 {
        self.shared.frames.load(Ordering::Relaxed)
    }

    /// Microphone level since the last call, 0..1 (before the monitor volume).
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.shared.peak.swap(0f32.to_bits(), Ordering::Relaxed))
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

fn explain(e: &windows::core::Error) -> String {
    // E_ACCESSDENIED: Windows privacy settings block desktop apps from the microphone.
    if e.code().0 as u32 == 0x8007_0005 {
        "Windows is blocking microphone access (Settings › Privacy & security › Microphone › let desktop apps access it)".into()
    } else {
        format!("couldn't open the microphone or the output: {}", e.message())
    }
}

struct Event(HANDLE);

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

unsafe fn session(mic: &str, shared: &Shared) -> windows::core::Result<()> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let mic_dev = en.GetDevice(&HSTRING::from(crate::sys::endpoint_id(mic, true)))?;
        let out_dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let out_id = out_dev.GetId()?.to_string().unwrap_or_default();

        let cap_client: IAudioClient = mic_dev.Activate(CLSCTX_ALL, None)?;
        let fmt = cap_client.GetMixFormat()?;
        let format = Format::read(fmt);
        // 20 ms: one engine period of slack on top of the one being filled.
        let init = cap_client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 200_000, 0, fmt, None);
        CoTaskMemFree(Some(fmt as _));
        init?;
        let event = Event(CreateEventW(None, false, false, None)?);
        cap_client.SetEventHandle(event.0)?;
        let cap: IAudioCaptureClient = cap_client.GetService()?;

        let out_client: IAudioClient = out_dev.Activate(CLSCTX_ALL, None)?;
        let block = (OUT_CHANNELS * 4) as u16;
        let wf = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
            nChannels: OUT_CHANNELS as u16,
            nSamplesPerSec: format.rate,
            nAvgBytesPerSec: format.rate * block as u32,
            nBlockAlign: block,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        out_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            1_000_000,
            0,
            &wf,
            None,
        )?;
        let capacity = out_client.GetBufferSize()?;
        let render: IAudioRenderClient = out_client.GetService()?;

        // A small cushion so a late packet doesn't click; more than `ceiling`
        // queued means the microphone runs fast, so a packet is skipped.
        let cushion = format.rate / 50;
        let ceiling = format.rate * 2 / 25;

        out_client.Start()?;
        cap_client.Start()?;
        let mut last_check = Instant::now();
        let mut samples: Vec<f32> = Vec::new();
        let mut stereo: Vec<f32> = Vec::new();
        while !shared.stop.load(Ordering::Relaxed) {
            WaitForSingleObject(event.0, 200);
            let mut n = cap.GetNextPacketSize()?;
            while n > 0 {
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let fr = frames as usize;
                samples.clear();
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() || format.channels == 0 {
                    samples.resize(fr * format.channels.max(1), 0.0);
                } else {
                    format.floats(data, fr, &mut samples);
                }
                cap.ReleaseBuffer(frames)?;

                let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));
                let mut peak = 0f32;
                stereo.clear();
                for f in samples.chunks_exact(format.channels.max(1)) {
                    // Mono goes to both ears; anything wider keeps its first two channels.
                    let (l, r) = if f.len() >= 2 { (f[0], f[1]) } else { (f[0], f[0]) };
                    peak = peak.max(l.abs()).max(r.abs());
                    stereo.push((l * volume).clamp(-1.0, 1.0));
                    stereo.push((r * volume).clamp(-1.0, 1.0));
                }
                let seen = f32::from_bits(shared.peak.load(Ordering::Relaxed));
                if peak > seen {
                    shared.peak.store(peak.to_bits(), Ordering::Relaxed);
                }

                let queued = out_client.GetCurrentPadding()?;
                if queued <= ceiling {
                    if queued == 0 {
                        let pad = cushion.min(capacity);
                        let p = render.GetBuffer(pad)?;
                        std::ptr::write_bytes(p, 0, pad as usize * block as usize);
                        render.ReleaseBuffer(pad, 0)?;
                    }
                    let free = capacity.saturating_sub(out_client.GetCurrentPadding()?);
                    let write = (stereo.len() / OUT_CHANNELS).min(free as usize);
                    if write > 0 {
                        let p = render.GetBuffer(write as u32)?;
                        std::ptr::copy_nonoverlapping(stereo.as_ptr() as *const u8, p, write * block as usize);
                        render.ReleaseBuffer(write as u32, 0)?;
                        shared.frames.fetch_add(write as u32, Ordering::Relaxed);
                    }
                }
                n = cap.GetNextPacketSize()?;
            }
            if last_check.elapsed() > Duration::from_secs(1) {
                last_check = Instant::now();
                let now = en
                    .GetDefaultAudioEndpoint(eRender, eConsole)
                    .and_then(|d| d.GetId())
                    .map(|p| p.to_string().unwrap_or_default());
                if now.ok().as_deref() != Some(out_id.as_str()) {
                    break;
                }
            }
        }
        let _ = cap_client.Stop();
        let _ = out_client.Stop();
        Ok(())
    }
}
