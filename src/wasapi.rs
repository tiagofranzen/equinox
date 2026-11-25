//! What the analyzer and the microphone monitor share: reading a WASAPI mix
//! format and turning its packets into float samples.

use windows::Win32::Media::Audio::{WAVEFORMATEX, WAVEFORMATEXTENSIBLE};

const TAG_PCM: u16 = 1;
const TAG_FLOAT: u16 = 3;
const TAG_EXTENSIBLE: u16 = 0xFFFE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sample {
    F32,
    I16,
    I24,
    I32,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
pub struct Format {
    pub channels: usize,
    pub rate: u32,
    pub sample: Sample,
}

impl Format {
    /// # Safety
    /// `fmt` must point at a valid `WAVEFORMATEX` (as returned by `GetMixFormat`).
    pub unsafe fn read(fmt: *const WAVEFORMATEX) -> Format {
        unsafe {
            let f = fmt.read_unaligned();
            let (tag, bits) = (f.wFormatTag, f.wBitsPerSample);
            // Extensible formats name the sample type by GUID; its first field is the classic tag.
            let tag = if tag == TAG_EXTENSIBLE && f.cbSize >= 22 {
                let sub = std::ptr::addr_of!((*(fmt as *const WAVEFORMATEXTENSIBLE)).SubFormat).read_unaligned();
                sub.data1 as u16
            } else {
                tag
            };
            let sample = match (tag, bits) {
                (TAG_FLOAT, 32) => Sample::F32,
                (TAG_PCM, 16) => Sample::I16,
                (TAG_PCM, 24) => Sample::I24,
                (TAG_PCM, 32) => Sample::I32,
                _ => Sample::Unknown,
            };
            Format {
                channels: f.nChannels as usize,
                rate: f.nSamplesPerSec,
                sample,
            }
        }
    }

    /// Append `frames` frames from a capture packet to `out` as interleaved
    /// floats, all channels kept. Unknown formats come out as silence.
    ///
    /// # Safety
    /// `data` must hold `frames` frames in this format.
    pub unsafe fn floats(&self, data: *const u8, frames: usize, out: &mut Vec<f32>) {
        let n = frames * self.channels;
        unsafe {
            match self.sample {
                Sample::F32 => out.extend_from_slice(std::slice::from_raw_parts(data as *const f32, n)),
                Sample::I16 => out.extend(
                    std::slice::from_raw_parts(data as *const i16, n)
                        .iter()
                        .map(|&v| v as f32 / 32768.0),
                ),
                Sample::I24 => out.extend(std::slice::from_raw_parts(data, n * 3).as_chunks::<3>().0.iter().map(|b| {
                    let v = i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8;
                    v as f32 / 8_388_608.0
                })),
                Sample::I32 => out.extend(
                    std::slice::from_raw_parts(data as *const i32, n)
                        .iter()
                        .map(|&v| v as f32 / 2_147_483_648.0),
                ),
                Sample::Unknown => out.resize(out.len() + n, 0.0),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_integer_samples() {
        let f = Format {
            channels: 2,
            rate: 48_000,
            sample: Sample::I16,
        };
        let pcm: [i16; 4] = [0, 16384, -32768, 32767];
        let mut out = Vec::new();
        unsafe { f.floats(pcm.as_ptr() as *const u8, 2, &mut out) };
        assert_eq!(out.len(), 4);
        assert!((out[1] - 0.5).abs() < 1e-6 && (out[2] + 1.0).abs() < 1e-6);

        let f24 = Format {
            channels: 1,
            rate: 48_000,
            sample: Sample::I24,
        };
        // 0x400000 = half scale, 0xC00000 = minus half scale.
        let bytes = [0x00u8, 0x00, 0x40, 0x00, 0x00, 0xC0];
        let mut out = Vec::new();
        unsafe { f24.floats(bytes.as_ptr(), 2, &mut out) };
        assert!((out[0] - 0.5).abs() < 1e-6 && (out[1] + 0.5).abs() < 1e-6, "{out:?}");
    }
}
