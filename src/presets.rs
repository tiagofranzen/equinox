//! Starter profiles, for outputs and for microphones. They're ordinary profiles once created: rename, edit or delete them.

use crate::model::{Band, FilterKind::*, Tone};

fn tone(bass: f32, warmth: f32, clarity: f32, air: f32) -> Tone {
    Tone {
        bass,
        warmth,
        clarity,
        air,
    }
}

pub fn builtin() -> Vec<(&'static str, Tone, Vec<Band>)> {
    vec![
        ("Flat", Tone::default(), vec![]),
        ("Bass Boost", tone(6.0, 1.0, 0.0, 0.0), vec![]),
        ("Warm", tone(2.5, 2.0, -1.0, -1.5), vec![]),
        ("Bright", tone(0.0, -1.0, 1.5, 3.5), vec![]),
        ("V-Shape", tone(4.5, -1.0, -1.5, 3.5), vec![]),
        ("Vocal", tone(-2.0, -1.0, 3.0, 1.0), vec![Band::new(HighPass, 70.0, 0.0, 0.707)]),
        (
            "Late Night",
            tone(-3.0, 0.0, 1.0, -2.0),
            vec![Band::new(Peak, 3500.0, -1.5, 1.2), Band::new(HighPass, 45.0, 0.0, 0.6)],
        ),
        (
            "Footsteps",
            tone(-4.0, -2.0, 2.0, 1.0),
            vec![Band::new(Peak, 2500.0, 3.0, 1.0), Band::new(Peak, 5000.0, 2.0, 1.4)],
        ),
        (
            "Treble Tame",
            Tone::default(),
            vec![Band::new(Peak, 6500.0, -3.0, 2.0), Band::new(HighShelf, 9000.0, -2.0, 0.7)],
        ),
        (
            "Laptop Speakers",
            tone(0.0, -1.5, 2.0, 1.5),
            vec![
                Band::new(HighPass, 140.0, 0.0, 0.707),
                Band::new(Peak, 180.0, 3.0, 1.5),
                Band::new(Peak, 450.0, -2.0, 1.4),
            ],
        ),
        ("Loudness", tone(5.0, 0.0, -1.0, 3.0), vec![]),
    ]
}

/// Starter profiles for microphones: voice shaping, on top of any mic correction.
pub fn builtin_mic() -> Vec<(&'static str, Tone, Vec<Band>)> {
    vec![
        ("Natural", Tone::default(), vec![]),
        (
            "Voice Clarity",
            tone(0.0, -1.5, 2.5, 1.0),
            vec![Band::new(HighPass, 80.0, 0.0, 0.707), Band::new(Peak, 400.0, -1.5, 1.2)],
        ),
        (
            "Broadcast",
            tone(2.5, -1.0, 2.0, 1.5),
            vec![
                Band::new(HighPass, 70.0, 0.0, 0.707),
                Band::new(Peak, 350.0, -2.5, 1.3),
                Band::new(Peak, 7500.0, -1.5, 3.0),
            ],
        ),
        ("Rumble Cut", Tone::default(), vec![Band::new(HighPass, 100.0, 0.0, 0.707)]),
        (
            "De-Mud",
            Tone::default(),
            vec![
                Band::new(HighPass, 85.0, 0.0, 0.707),
                Band::new(Peak, 250.0, -3.0, 1.1),
                Band::new(Peak, 500.0, -1.5, 1.4),
            ],
        ),
        (
            "Tame Harshness",
            Tone::default(),
            vec![
                Band::new(HighPass, 75.0, 0.0, 0.707),
                Band::new(Peak, 3200.0, -2.0, 1.6),
                Band::new(Peak, 7000.0, -3.0, 2.5),
            ],
        ),
        (
            "Headset Mic Fix",
            tone(3.0, 1.5, 0.0, 2.0),
            vec![
                Band::new(HighPass, 90.0, 0.0, 0.707),
                Band::new(Peak, 1100.0, -2.5, 1.3),
                Band::new(Peak, 3000.0, -2.0, 1.5),
            ],
        ),
        (
            "Deep Voice",
            tone(4.0, 1.0, 1.0, 0.0),
            vec![Band::new(HighPass, 60.0, 0.0, 0.707), Band::new(Peak, 300.0, -1.5, 1.4)],
        ),
    ]
}
