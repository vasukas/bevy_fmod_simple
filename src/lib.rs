//! Simple FMOD (audio engine) plugin for bevy
//!
//! Features:
//! - entity-based API;
//! - playback control: volume and speed;
//! - 3D spatial audio:
//!     - distance falloff and Doppler effect;
//!     - occlusion by geometry;
//!     - reverb effect;
//! - support for most common audio file formats;
//! - sound groups and global settings.
//!
//! Missing features:
//! - per-group DSP;
//! - support for procedurally-generated sounds;
//! - loop start and end points for looped sounds.

mod bridge;
mod plugin;

pub use plugin::*;
