/// Declarations for Rust <-> C++ bridge
#[cxx::bridge]
pub mod bridge {
    // Only bridge-specific stuff is documented here, for details see how bridge is
    // used in plugin or on C++ side.
    //
    // With the exception of sound priority all parameters mean the same thing in
    // Rust and in C++.

    /// 3D vector
    #[derive(Clone, Default)]
    struct Vector {
        x: f32,
        y: f32,
        z: f32,
    }

    struct InitParams {
        max_virtual_channels: i32,
        max_active_channels: i32,
    }

    struct EngineParams {
        doppler_scale: f32,
        distance_scale: f32,
        rolloff_scale: f32,
        max_world_size: f32,
    }

    struct GroupParams {
        user_id: i32,
        volume: f32,
    }

    #[derive(Default)]
    struct AudioFileParams<'a> {
        /// Path to the file, full or relative to current directory.
        /// This loads file as streaming.
        ///
        /// If defaulted, `file_contents` is used.
        filename: String,

        /// File fully loaded into memory.
        ///
        /// If defaulted, `custom` is used.
        file_contents: &'a [u8],
    }

    struct ChannelParams {
        /// ID of loaded/streamed sound
        file_id: i32,
        /// Group (user ID) to which sound belongs
        group_id: i32,
        /// Range `[0; 256]`. Lower number means higher priority
        priority: i32,

        // spatial parameters
        /// Sound is spatial - position and velocity are used.
        /// This can't be changed later.
        is_positional: bool,
        position: Vector,
        /// per second (used for doppler)
        velocity: Vector,
        min_distance: f32,
        max_distance: f32,

        // common parameters
        /// Loop playback infinitely
        looped: bool,
        /// Volume at which to play
        volume: f32,
        /// Speed at which to play (this IS playback speed, not pitch!)
        pitch: f32,

        /// Pause before actually starting playback, microseconds
        startup_delay: i32,
    }

    #[derive(Default)]
    struct ChannelUpdateParams {
        // spatial parameters
        /// If true, set new world position and velocity (spatial-only)
        set_position: bool,
        position: Vector,
        velocity: Vector,

        // common parameters
        /// If true, set new volume and other parameters
        set_volume_etc: bool,
        volume: f32,
        pitch: f32,
        priority: i32,
    }

    #[derive(Clone, Default)]
    struct ListenerParams {
        // World vectors for listener
        position: Vector,
        velocity: Vector, // per second (used for doppler)
        forward: Vector,  // unit (direction)
        up: Vector,       // unit (direction)
    }

    struct Polygon {
        /// All vertices of a 3D polygon.
        /// *Must* lay on same plane. *Must* be convex.
        vertices: Vec<Vector>,
    }

    struct Geometry {
        direct_occlusion: f32,
        reverb_occlusion: f32,
        polygons: Vec<Polygon>,
    }

    #[derive(Clone)]
    struct Reverb {
        min_dist: f32,
        max_dist: f32,
        /// World center of the sphere where effect is applied
        position: Vector,

        decay_time: f32,
        early_delay: f32,
        late_delay: f32,
        hf_reference: f32,
        hf_decay_ratio: f32,
        diffusion: f32,
        density: f32,
        low_shelf_frequency: f32,
        low_shelf_gain: f32,
        high_cut: f32,
        early_late_mix: f32,
        wet_level: f32,
    }

    // Rust methods visible in C++
    extern "Rust" {
        fn bridge_log_info(s: &[u8]);
        fn bridge_log_error(s: &[u8]);
    }

    // Interface class.
    // See `src-cpp\bridge.h`
    unsafe extern "C++" {
        include!("bevy_fmod_simple/src-cpp/bridge.h");

        type Bridge;

        // IDs can be same between object types, and are reused after being freed.
        //
        // All errors are logged; methods that return IDs will return -1 on failure.
        //
        // Some methods will crash the application if used incorrectly (i.e. using
        // invalid ID), but should never do it in any other situtation.

        fn create(params: InitParams) -> UniquePtr<Bridge>;
        fn update(self: Pin<&mut Bridge>); // must be called periodically
        fn update_engine(self: Pin<&mut Bridge>, params: EngineParams);

        fn update_listener(self: Pin<&mut Bridge>, params: ListenerParams);
        fn update_group(self: Pin<&mut Bridge>, params: GroupParams);

        fn load_audio_file(self: Pin<&mut Bridge>, params: AudioFileParams) -> i32; // returns -1 on error
        fn free_audio_file(self: Pin<&mut Bridge>, id: i32);

        fn play_channel(self: Pin<&mut Bridge>, params: ChannelParams) -> i32; // returns -1 on error
        fn update_channel(self: Pin<&mut Bridge>, id: i32, params: ChannelUpdateParams) -> bool;
        fn is_playing_channel(self: Pin<&mut Bridge>, id: i32) -> bool; // sound haven't stopped yet
        fn free_channel(self: Pin<&mut Bridge>, id: i32);

        fn add_geometry(self: Pin<&mut Bridge>, params: Geometry) -> i32; // returns -1 on error
        fn free_geometry(self: Pin<&mut Bridge>, id: i32);

        fn add_reverb(self: Pin<&mut Bridge>, params: Reverb) -> i32; // returns -1 on error
        fn free_reverb(self: Pin<&mut Bridge>, id: i32);
    }
}

// FMOD API is supposed to be thread-safe: https://documentation.help/FMOD-Studio-API/whatsnew_103.html
unsafe impl Send for bridge::Bridge {}
unsafe impl Sync for bridge::Bridge {}

fn bridge_log_info(s: &[u8]) {
    bevy::log::info!("{}", String::from_utf8_lossy(s));
}

fn bridge_log_error(s: &[u8]) {
    bevy::log::error!("{}", String::from_utf8_lossy(s));
}

impl From<bevy::prelude::Vec3> for bridge::Vector {
    fn from(v: bevy::prelude::Vec3) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}
