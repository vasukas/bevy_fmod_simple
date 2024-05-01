use super::bridge::bridge;
use bevy::{
    asset::AsyncReadExt as _,
    prelude::*,
    reflect::TypePath,
    transform::TransformSystem,
    utils::{HashMap, HashSet},
};
use std::{sync::Mutex, time::Duration};

#[cfg(feature = "randomize")]
use rand::prelude::*;

/// Add [`Handle<AudioSource>`] component to play sound.
///
/// If entity has [`GlobalTransform`], sound will be played as spatial
/// (relative to an entity with [`AudioListener`]).
///
/// Spatial sounds have distance falloff, panning and are affected by other
/// spatial entities such as reverb zones and geometry.
///
/// When playback stops, the entity will be despawned. Vice-versa, removing
/// [`Handle<AudioSource>`] stops playback.
#[derive(Asset, TypePath)]
pub struct AudioSource {
    id: EngineId,

    /// Default parameters, used only if that component is not present
    /// when handle is added to an entity. Component won't be added to the
    /// entity.
    pub params: AudioParameters,

    /// Randomize default parameters on each use
    #[cfg(feature = "randomize")]
    pub randomize_params: bool,
}

impl AudioSource {
    /// Load source from file loaded into memory.
    ///
    /// Returns [`None`] on error.
    ///
    /// This is how sounds are loaded via [`AssetServer`].
    pub fn from_memory(file_contents: &[u8]) -> Option<Self> {
        let mut bridge = BRIDGE.lock().unwrap();
        let bridge = bridge.as_mut().unwrap().pin_mut();
        let instance = bridge.load_audio_file(bridge::AudioFileParams {
            file_contents,
            ..default()
        });
        (instance != -1).then_some(Self::new(instance))
    }

    /// Stream file from disk as it is being played instead of loading it whole
    /// into memory first.
    ///
    /// _This is useful for music, since usually only one instance is needed,
    /// and uncompressed file can take a lot of memory._
    ///
    /// **Filename must be relative to current directory, not assets
    /// directory!**
    ///
    /// **Only one such source can be played back at once!**
    ///
    /// Returns [`None`] on error.
    pub fn stream_file(filename: String) -> Option<Self> {
        let mut bridge = BRIDGE.lock().unwrap();
        let bridge = bridge.as_mut().unwrap().pin_mut();
        let instance = bridge.load_audio_file(bridge::AudioFileParams {
            filename,
            ..default()
        });
        (instance != -1).then_some(Self::new(instance))
    }

    fn new(id: EngineId) -> Self {
        Self {
            id,
            params: default(),

            #[cfg(feature = "randomize")]
            randomize_params: false,
        }
    }

    fn params(&self) -> AudioParameters {
        #[cfg(feature = "randomize")]
        {
            let mut params = self.params;
            if self.randomize_params {
                params.randomize();
            }
            params
        }

        #[cfg(not(feature = "randomize"))]
        self.params
    }

    // TODO(later): implement custom audio source via trait object
}

impl Drop for AudioSource {
    fn drop(&mut self) {
        let mut bridge = BRIDGE.lock().unwrap();
        let bridge = bridge.as_mut().unwrap().pin_mut();
        bridge.free_audio_file(self.id);
    }
}

/// Add together with [`Handle<AudioSource>`] to play sound on repeat forever.
///
/// Otherwise this component is ignored.
// TODO(later): don't ignore changes.
#[derive(Component, Clone, Copy, Default)]
pub struct AudioLoop;

/// Add/change at any time to control playback.
#[derive(Component, Clone, Copy)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct AudioParameters {
    /// Linear volume multiplier; will be multiplied by group and master
    /// volumes.
    ///
    /// Should be in `[0; 1]` range. Value is not clamped.
    pub volume: f32,

    /// Playback speed multiplier, also changes pitch. Value is not clamped.
    pub speed: f32,

    /// If there is not enough free channels, sounds with higher priority will
    /// be played instead of low priority sounds.
    ///
    /// Lower value means higher priority.
    pub priority: u8,

    /// For spatial sound only: if distance from listener to sound is less,
    /// volume is max. Value is not clamped.
    ///
    /// **Used only when component is added together with
    /// [`Handle<AudioSource>`], later changes are ignored!**
    pub min_distance: f32,

    /// For spatial sound only: if distance from listener to sound is more,
    /// volume is zero. Value is not clamped.
    ///
    /// **Used only when component is added together with
    /// [`Handle<AudioSource>`], later changes are ignored!**
    pub max_distance: f32,
}

impl Default for AudioParameters {
    fn default() -> Self {
        Self {
            volume: 1.,
            speed: 1.,
            priority: 128,
            min_distance: 0.8,
            max_distance: 20.,
        }
    }
}

impl AudioParameters {
    /// Randomly change values a bit
    #[cfg(feature = "randomize")]
    pub fn randomize(&mut self) {
        self.volume *= thread_rng().gen_range(0.95..1.05);
        self.speed *= thread_rng().gen_range(0.95..1.05);
    }

    /// Randomly change values a bit
    #[cfg(feature = "randomize")]
    pub fn get_randomized(mut self) -> Self {
        self.randomize();
        self
    }
}

/// Add together with [`Handle<AudioSource>`] to start playback after specified
/// delay.
#[derive(Component, Clone, Default)]
pub struct AudioStartupDelay(pub Duration);

impl AudioStartupDelay {
    /// Set to small randomized delay (<= 10 ms)
    #[cfg(feature = "randomize")]
    pub fn random() -> Self {
        let max = 0.010; // 10 ms
        Self(Duration::from_secs_f32(thread_rng().gen_range(0. ..max)))
    }

    /// Randomly change value a bit
    #[cfg(feature = "randomize")]
    pub fn randomize(mut self) -> Self {
        let k = thread_rng().gen_range(0.95..1.05);
        self.0 = Duration::from_secs_f32(self.0.as_secs_f32() * k);
        self
    }
}

/// Add together with [`Handle<AudioSource>`] to assign sound to a non-default
/// group.
///
/// Otherwise this component is ignored.
///
/// Each sound is assigned to a group, for easier manipulation.
/// Groups are defined by user (except for default group `AudioGroup(0)`)
///
/// Groups are not required to be registered in any way.
/// ATM they are used only for per-group settings, but there are plans for
/// per-group effect plugins and combining several groups.
// TODO(later): dont' ignore changes
#[derive(Component, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct AudioGroup(pub i32);

/// Add audio geometry to the engine to occlude spatial sounds.
/// Removal of this component removes geometry from the engine.
///
/// Otherwise this component is ignored.
///
/// Requires [`GlobalTransform`]. Changes to it will be ignored.
// TODO(later): dont' ignore changes
#[derive(Component, Clone, Default)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct AudioGeometry {
    pub polygon_vertices: AudioGeometryData,
    pub params: AudioGeometryParams,
}

/// Vec of planar polygons - each polygon can have any number of points,
/// but they must lie on the same plane.
///
/// Polygon must be convex.
pub type AudioGeometryData = Vec<Vec<Vec3>>;

/// Parameters for audio geometry
#[derive(Clone, Copy, Debug)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct AudioGeometryParams {
    /// Volume of non-reverberated part of sound behind the geometry, in `[0;
    /// 1]` range.
    pub direct_occlusion: f32,

    /// Volume of reverberated part of sound (when geometry is between the sound
    /// and the center of the reverb sphere), in `[0; 1]` range.
    pub reverb_occlusion: f32,
}

impl Default for AudioGeometryParams {
    fn default() -> Self {
        Self {
            direct_occlusion: 0.3,
            reverb_occlusion: 0.3,
        }
    }
}

/// Add reverb sphere to the engine to affect spatial sounds.
/// Removal of this component removes reverb from the engine.
///
/// Otherwise this component is ignored.
///
/// Requires [`GlobalTransform`]. Changes to it will be ignored.
// TODO(later): dont' ignore changes
#[derive(Component, Debug)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct AudioReverbSphere {
    /// Effect is applied in full to sounds closer than that
    pub min_distance: f32,

    /// Effect is not applied to sounds farther than that
    pub max_distance: f32,

    pub props: AudioReverbProps,
}

impl Default for AudioReverbSphere {
    fn default() -> Self {
        Self {
            min_distance: 5.,
            max_distance: 20.,
            ..default()
        }
    }
}

/// Reverb properties
#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct AudioReverbProps {
    /// Reverberation decay time.
    ///
    /// Milliseconds, range `[0; 20_000]`.
    pub decay_time: f32,

    /// Initial reflection delay time.
    ///
    /// Milliseconds, range `[0; 300]`.
    pub early_delay: f32,

    /// Late reverberation delay time relative to initial reflection.
    ///
    /// Milliseconds, range `[0; 100]`.
    pub late_delay: f32,

    /// Reference high frequency.
    ///
    /// Hertz, range `[20; 20_000]`.
    pub hf_reference: f32,

    /// High-frequency to mid-frequency decay time ratio.
    ///
    /// Percent, range `[10; 100]`.
    pub hf_decay_ratio: f32,

    /// Value that controls the echo density in the late reverberation decay.
    ///
    /// Percent, range `[10; 100]`.
    pub diffusion: f32,

    /// Value that controls the modal density in the late reverberation decay.
    ///
    /// Percent, range `[10; 100]`.
    pub density: f32,

    /// Reference low frequency.
    ///
    /// Hertz, range `[20; 1000]`.
    pub low_shelf_frequency: f32,

    /// Relative room effect level at low frequencies.
    ///
    /// Decibels, range `[-36, 12]`.
    pub low_shelf_gain: f32,

    /// Relative room effect level at high frequencies.
    ///
    /// Hertz, range `[0; 200_000]`.
    pub high_cut: f32,

    /// Early reflections level relative to room effect.
    ///
    /// Percent, range `[0; 100]`.
    pub early_late_mix: f32,

    /// Room effect level at mid frequencies.
    ///
    /// Decibels, range `[-80; 20]`.
    pub wet_level: f32,
}

impl Default for AudioReverbProps {
    // `FMOD_PRESET_GENERIC`
    fn default() -> Self {
        Self {
            decay_time: 1500.,
            early_delay: 7.,
            late_delay: 11.,
            hf_reference: 5000.,
            hf_decay_ratio: 50.,
            diffusion: 50.,
            density: 100.,
            low_shelf_frequency: 250.,
            low_shelf_gain: 0.,
            high_cut: 200_000.,
            early_late_mix: 50.,
            wet_level: -6.,
        }
    }
}

impl AudioReverbProps {
    /// `FMOD_PRESET_HALLWAY`, sounds like somewhat wide corridor
    pub fn hallway() -> Self {
        Self {
            decay_time: 1500.,
            early_delay: 7.,
            late_delay: 11.,
            hf_reference: 5000.,
            hf_decay_ratio: 59.,
            diffusion: 100.,
            density: 100.,
            low_shelf_frequency: 250.,
            low_shelf_gain: 0.,
            high_cut: 7800.,
            early_late_mix: 87.,
            wet_level: -5.5,
        }
    }

    /// `FMOD_PRESET_HANGAR`, sounds like giant empty room
    pub fn hangar() -> Self {
        Self {
            decay_time: 10000.,
            early_delay: 20.,
            late_delay: 30.,
            hf_reference: 5000.,
            hf_decay_ratio: 23.,
            diffusion: 100.,
            density: 100.,
            low_shelf_frequency: 250.,
            low_shelf_gain: 0.,
            high_cut: 3400.,
            early_late_mix: 72.,
            wet_level: -7.4,
        }
    }

    /// Exaggerated reverb for giant empty room
    pub fn huge_room() -> Self {
        Self {
            decay_time: 6000.,
            wet_level: 3.,
            ..Self::hangar()
        }
    }
}

/// Marker for entity whose position is used for spatial
/// audio.
///
/// Requires [`GlobalTransform`].
///
/// There can't be multiple listeners.
///
/// If listener doesn't exist, spatial sounds will play at the last remembered
/// position (which is `Vec3::ZERO` on startup).
#[derive(Component, Clone, Default)]
pub struct AudioListener;

/// Global engine settings
#[derive(Resource, Clone, Debug)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct AudioSettings {
    /// Per-group settings.
    ///
    /// If group isn't present here, defaults will be used for sounds belonging
    /// to that group.
    pub groups: HashMap<AudioGroup, AudioGroupParameters>,

    /// Linear volume multiplier applied to all sounds.
    ///
    /// Should be in `[0; 1]` range.
    pub master_volume: f32,

    /// If false, consider master volume to be zero.
    ///
    /// _Hearing same sounds and music over-and-over-and-over-again in long
    /// debugging sessions gets really, really annoying, doesn't it?_
    pub enabled: bool,

    pub engine: AudioEngineSettings,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            groups: default(),
            master_volume: 0.5,
            enabled: true,
            engine: default(),
        }
    }
}

/// Per-group engine settings
#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct AudioGroupParameters {
    /// Linear volume multiplier for all sounds in the group.
    ///
    /// Should be in `[0; 1]` range.
    pub volume: f32,
}

impl Default for AudioGroupParameters {
    fn default() -> Self {
        Self { volume: 1. }
    }
}

/// Global engine configuration
#[derive(Resource, Clone, Debug)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct AudioEngineSettings {
    /// How much pitch varies with relative speed (Doppler effect).
    ///
    /// With this at 1 effective sound speed is 340 m/s.
    pub doppler_scale: f32,

    /// Used only for doppler. Set to 1 if you use meters, set to 3.28 if you
    /// use feet.
    pub distance_scale: f32,

    /// Global factor applied to all distance calculations:
    ///
    /// `distance = (distance - minDistance) * rolloffscale + minDistance`
    pub rolloff_scale: f32,

    /// Expected max coordinate values.
    ///
    /// _This isn't a hard limitation, but apparently exceeding it results in
    /// worse performance._
    pub max_world_size: f32,
}

impl Default for AudioEngineSettings {
    fn default() -> Self {
        Self {
            doppler_scale: 0.33,
            distance_scale: 1.,
            rolloff_scale: 1.,
            max_world_size: 500.,
        }
    }
}

//
// plugin
//

/// All systems are executed in this set in [`PostUpdate`]
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct AudioSystem;

/// File extensions of supported audio files, lowercase without leading dot.
///
/// _Actually more types are supported, but why would you use anything else?_
pub const AUDIO_FILE_EXTENSIONS: &'static [&'static str] = &["flac", "mp3", "ogg", "wav"];

/// Engine configuration which cannot be changed after initialization
#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serialize",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct AudioEngineInitSettings {
    /// How many sounds may exist at once.
    ///
    /// Only active ones will be played, based on priority and calculated
    /// volume. Max value is `4095`.
    pub max_virtual_channels: usize,

    /// How many sounds can be played at once.
    ///
    /// If there are more sounds than active channels, sounds with lower
    /// priority will be muted.
    ///
    /// Must be lower than `max_virtual_channels`.
    pub max_active_channels: usize,
}

impl Default for AudioEngineInitSettings {
    fn default() -> Self {
        Self {
            max_virtual_channels: 1024,
            max_active_channels: 32,
        }
    }
}

/// Audio engine and all related systems
#[derive(Default)]
pub struct FmodAudioPlugin {
    pub settings: AudioEngineInitSettings,
}

impl Plugin for FmodAudioPlugin {
    fn build(&self, app: &mut App) {
        // TODO(later): allow re-init of everything

        *BRIDGE.lock().unwrap() = {
            let p = bridge::create(bridge::InitParams {
                max_virtual_channels: self.settings.max_virtual_channels.min(4095) as i32,
                max_active_channels: self
                    .settings
                    .max_active_channels
                    .min(self.settings.max_virtual_channels)
                    as i32,
            });
            // TODO(later): allow bridge to be None
            if p.is_null() {
                panic!("Failed to initialize audio");
            }
            Some(p)
        };

        app.configure_sets(PostUpdate, AudioSystem)
            .init_resource::<AudioSettings>()
            .init_asset::<AudioSource>()
            .register_asset_loader(AudioFileLoader);

        // system update
        app.add_systems(
            PostUpdate,
            (
                update_listener.after(TransformSystem::TransformPropagate),
                update_system.after(update_listener),
                update_engine_settings
                    .before(update_system)
                    .run_if(resource_changed::<AudioSettings>),
            )
                .in_set(AudioSystem),
        );

        // playback
        app.init_resource::<AudioInstanceMapping>().add_systems(
            PostUpdate,
            (
                play_audio
                    .before(update_engine_settings)
                    .after(TransformSystem::TransformPropagate),
                stop_audio,
                detect_stopped_audio,
                update_spatial_audio.after(TransformSystem::TransformPropagate),
                update_audio_parameters,
            )
                .in_set(AudioSystem)
                .before(update_system),
        );

        // geometry
        app.init_resource::<GeometryInstanceMapping>().add_systems(
            PostUpdate,
            (
                add_geometry.after(TransformSystem::TransformPropagate),
                remove_geometry,
            )
                .in_set(AudioSystem),
        );

        // reverb
        app.init_resource::<ReverbInstanceMapping>().add_systems(
            PostUpdate,
            (
                add_reverb.after(TransformSystem::TransformPropagate),
                remove_reverb,
            )
                .in_set(AudioSystem),
        );
    }
}

lazy_static::lazy_static! {
    /// Engine instance (C++ wrapper)
    static ref BRIDGE: Mutex<Option<cxx::UniquePtr<bridge::Bridge>>> = default();
}

/// IDs used for sounds, channels and spatial objects
type EngineId = i32;

//
// assets

struct AudioFileLoader;

impl bevy::asset::AssetLoader for AudioFileLoader {
    type Asset = AudioSource;
    type Settings = ();
    type Error = String;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        _settings: &'a Self::Settings,
        _load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = vec![];
            reader
                .read_to_end(&mut bytes)
                .await
                .map_err(|e| format!("failed to load file: {e}"))?;
            AudioSource::from_memory(&bytes).ok_or_else(|| "failed to parse file".to_string())
        })
    }

    fn extensions(&self) -> &[&str] {
        AUDIO_FILE_EXTENSIONS
    }
}

//
// system update

struct ListenerData {
    data: bridge::ListenerParams,
    old_position: Option<Vec3>,
}

impl Default for ListenerData {
    fn default() -> Self {
        Self {
            data: bridge::ListenerParams {
                forward: Vec3::NEG_Z.into(),
                up: Vec3::Y.into(),
                ..default()
            },
            old_position: None,
        }
    }
}

fn update_listener(
    listener_entity: Query<&GlobalTransform, With<AudioListener>>,
    mut listener: Local<ListenerData>,
    time: Res<Time>,
) {
    if let Ok(transform) = listener_entity.get_single() {
        let position = transform.translation();
        let velocity = if time.delta() != default() {
            (position - listener.old_position.unwrap_or(position)) / time.delta_seconds()
        } else {
            Vec3::ZERO
        };
        listener.old_position = position.into();

        let listener = &mut listener.data;
        listener.position = position.into();
        listener.velocity = velocity.into();
        listener.forward = transform.forward().into();
        listener.up = transform.up().into();
    } else {
        listener.data.velocity = default();
        listener.old_position = None;
    }

    BRIDGE
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .pin_mut()
        .update_listener(listener.data.clone());
}

fn update_system() {
    BRIDGE.lock().unwrap().as_mut().unwrap().pin_mut().update();
}

fn update_engine_settings(settings: Res<AudioSettings>) {
    let mut bridge = BRIDGE.lock().unwrap();
    let bridge = bridge.as_mut().unwrap();

    let master_volume = settings
        .enabled
        .then_some(settings.master_volume)
        .unwrap_or(0.);

    for (id, params) in settings.groups.iter() {
        bridge.pin_mut().update_group(bridge::GroupParams {
            user_id: id.0,
            volume: params.volume * master_volume,
        })
    }

    let engine = &settings.engine;
    bridge.pin_mut().update_engine(bridge::EngineParams {
        doppler_scale: engine.doppler_scale,
        distance_scale: engine.distance_scale,
        rolloff_scale: engine.rolloff_scale,
        max_world_size: engine.max_world_size,
    });
}

//
// playback

#[derive(Resource, Default)]
struct AudioInstanceMapping {
    ids: HashMap<Entity, EngineId>,
    just_removed: HashSet<Entity>,
}

/// Sound currently being played
#[derive(Component)]
struct AudioInstance {
    id: EngineId,

    /// For spatial: position in previous frame
    old_position: Vec3,

    /// Ensure handle always outlives the sound
    _source: Handle<AudioSource>,
}

fn play_audio(
    new_audio: Query<
        (
            Entity,
            &Handle<AudioSource>,
            Option<&GlobalTransform>,
            Option<&AudioLoop>,
            Option<&AudioParameters>,
            Option<&AudioStartupDelay>,
            Option<&AudioGroup>,
        ),
        Added<Handle<AudioSource>>,
    >,
    sounds: Res<Assets<AudioSource>>,
    mut commands: Commands,
    mut mapping: ResMut<AudioInstanceMapping>,
) {
    let mut bridge = BRIDGE.lock().unwrap();
    let bridge = bridge.as_mut().unwrap();

    for (entity, source, transform, looped, parameters, startup_delay, group) in new_audio.iter() {
        let Some(mut commands) = commands.get_entity(entity) else {
            continue;
        };

        let looped = looped.is_some();

        let sound = match sounds.get(source) {
            Some(v) => v,
            None => {
                warn!("AudioSource asset {source:?} not loaded yet! Sound won't be played");
                if !looped {
                    commands.despawn_recursive();
                }
                continue;
            }
        };

        let parameters = parameters.copied().unwrap_or_else(|| sound.params());
        let position = transform.map(|t| t.translation()).unwrap_or(Vec3::ZERO);

        let instance = bridge.pin_mut().play_channel(bridge::ChannelParams {
            file_id: sound.id,
            group_id: group.copied().unwrap_or_default().0,
            priority: parameters.priority as i32,
            is_positional: transform.is_some(),
            position: position.into(),
            velocity: Vec3::ZERO.into(),
            min_distance: parameters.min_distance,
            max_distance: parameters.max_distance,
            looped,
            volume: parameters.volume,
            pitch: parameters.speed,
            startup_delay: startup_delay.map(|v| v.0).unwrap_or_default().as_micros() as i32,
        });

        if instance == -1 {
            if !looped {
                commands.despawn_recursive();
            }
            continue;
        }

        commands.insert(AudioInstance {
            id: instance,
            old_position: position,
            _source: source.clone(),
        });
        mapping.ids.insert(entity, instance);
    }
}

// entity was despawned, stop the sound
fn stop_audio(
    mut removed: RemovedComponents<Handle<AudioSource>>,
    mut mapping: ResMut<AudioInstanceMapping>,
    mut commands: Commands,
) {
    let mut bridge = BRIDGE.lock().unwrap();
    let bridge = bridge.as_mut().unwrap();

    for entity in removed.read() {
        let just_removed = mapping.just_removed.remove(&entity);
        match mapping.ids.remove(&entity) {
            Some(instance) => {
                if let Some(mut commands) = commands.get_entity(entity) {
                    commands.remove::<AudioInstance>();
                }
                bridge.pin_mut().free_channel(instance);
            }
            None => {
                if !just_removed {
                    error!("removing non-existent sound for entity {entity:?}")
                }
            }
        }
    }
}

// sound stopped, despawn the entity
fn detect_stopped_audio(mut mapping: ResMut<AudioInstanceMapping>, mut commands: Commands) {
    let mut bridge = BRIDGE.lock().unwrap();
    let bridge = bridge.as_mut().unwrap();

    let mapping = &mut *mapping;
    mapping.ids.retain(|entity, instance| {
        let keep = bridge.pin_mut().is_playing_channel(*instance);
        if !keep {
            if let Some(commands) = commands.get_entity(*entity) {
                commands.despawn_recursive();
            }
            bridge.pin_mut().free_channel(*instance);
            mapping.just_removed.insert(*entity);
        }
        keep
    });
}

fn update_spatial_audio(
    mut sounds: Query<(&GlobalTransform, &mut AudioInstance)>,
    time: Res<Time>,
) {
    let mut bridge = BRIDGE.lock().unwrap();
    let bridge = bridge.as_mut().unwrap();

    for (transform, mut instance) in sounds.iter_mut() {
        let position = transform.translation();
        let velocity = if time.delta() != default() {
            (position - instance.old_position) / time.delta_seconds()
        } else {
            Vec3::ZERO
        };
        instance.old_position = position.into();

        bridge.pin_mut().update_channel(
            instance.id,
            bridge::ChannelUpdateParams {
                set_position: true,
                position: position.into(),
                velocity: velocity.into(),
                ..default()
            },
        );
    }
}

fn update_audio_parameters(
    sounds: Query<(&AudioParameters, &AudioInstance), Changed<AudioParameters>>,
) {
    let mut bridge = BRIDGE.lock().unwrap();
    let bridge = bridge.as_mut().unwrap();

    for (parameters, instance) in sounds.iter() {
        bridge.pin_mut().update_channel(
            instance.id,
            bridge::ChannelUpdateParams {
                set_volume_etc: true,
                volume: parameters.volume,
                pitch: parameters.speed,
                priority: parameters.priority as i32,
                ..default()
            },
        );
    }
}

//
// geometry

#[derive(Resource, Default)]
struct GeometryInstanceMapping(HashMap<Entity, EngineId>);

fn add_geometry(
    new_geometries: Query<(Entity, &AudioGeometry, &GlobalTransform), Added<AudioGeometry>>,
    mut mapping: ResMut<GeometryInstanceMapping>,
) {
    let mut bridge = BRIDGE.lock().unwrap();
    let bridge = bridge.as_mut().unwrap();

    for (entity, geometry, transform) in new_geometries.iter() {
        let instance = bridge.pin_mut().add_geometry(bridge::Geometry {
            direct_occlusion: geometry.params.direct_occlusion.clamp(0., 1.),
            reverb_occlusion: geometry.params.reverb_occlusion.clamp(0., 1.),
            polygons: geometry
                .polygon_vertices
                .iter()
                .map(|polygon| bridge::Polygon {
                    vertices: polygon
                        .iter()
                        .map(|vertex| (*transform * *vertex).into())
                        .collect(),
                })
                .collect(),
        });
        if instance == -1 {
            error!("failed to create geometry object for {entity:?}");
            continue;
        }
        mapping.0.insert(entity, instance);
    }
}

fn remove_geometry(
    mut removed: RemovedComponents<AudioGeometry>,
    mut mapping: ResMut<GeometryInstanceMapping>,
) {
    let mut bridge = BRIDGE.lock().unwrap();
    let bridge = bridge.as_mut().unwrap();

    for entity in removed.read() {
        match mapping.0.remove(&entity) {
            Some(id) => bridge.pin_mut().free_geometry(id),
            None => error!("removing non-existent geometry for entity {entity:?}"),
        }
    }
}

//
// reverb

#[derive(Resource, Default)]
struct ReverbInstanceMapping(HashMap<Entity, EngineId>);

fn add_reverb(
    new_reverbs: Query<(Entity, &AudioReverbSphere, &GlobalTransform), Added<AudioReverbSphere>>,
    mut mapping: ResMut<ReverbInstanceMapping>,
) {
    let mut bridge = BRIDGE.lock().unwrap();
    let bridge = bridge.as_mut().unwrap();

    for (entity, reverb, transform) in new_reverbs.iter() {
        let instance = bridge.pin_mut().add_reverb(bridge::Reverb {
            min_dist: reverb.min_distance,
            max_dist: reverb.max_distance,
            position: transform.translation().into(),

            decay_time: reverb.props.decay_time,
            early_delay: reverb.props.early_delay,
            late_delay: reverb.props.late_delay,
            hf_reference: reverb.props.hf_reference,
            hf_decay_ratio: reverb.props.hf_decay_ratio,
            diffusion: reverb.props.diffusion,
            density: reverb.props.density,
            low_shelf_frequency: reverb.props.low_shelf_frequency,
            low_shelf_gain: reverb.props.low_shelf_gain,
            high_cut: reverb.props.high_cut,
            early_late_mix: reverb.props.early_late_mix,
            wet_level: reverb.props.wet_level,
        });
        if instance == -1 {
            error!("failed to create reverb object for entity {entity:?}");
            continue;
        }
        mapping.0.insert(entity, instance);
    }
}

fn remove_reverb(
    mut removed: RemovedComponents<AudioReverbSphere>,
    mut mapping: ResMut<ReverbInstanceMapping>,
) {
    let mut bridge = BRIDGE.lock().unwrap();
    let bridge = bridge.as_mut().unwrap();

    for entity in removed.read() {
        match mapping.0.remove(&entity) {
            Some(id) => bridge.pin_mut().free_reverb(id),
            None => error!("removing non-existent reverb for entity {entity:?}"),
        }
    }
}
