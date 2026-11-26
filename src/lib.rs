//! equinox: a system-wide equalizer for Windows, driving Equalizer APO.
//! Shared by the app (`equinox.exe`) and the command line (`eqx.exe`).

pub mod analyzer;
pub mod apo;
pub mod apo_setup;
pub mod autoeq;
pub mod devices;
pub mod dsp;
pub mod effectpack;
pub mod engine;
pub mod enhance;
pub mod eqtext;
pub mod fetch;
pub mod icon;
pub mod miccheck;
pub mod miclib;
pub mod model;
pub mod monitor;
pub mod paths;
pub mod presets;
pub mod sys;
pub mod update;
pub mod wasapi;
