use bevy::{
    app::AppExit,
    asset::LoadState,
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::MouseMotion,
    prelude::*,
    render::mesh::PlaneMeshBuilder,
    window::{CursorGrabMode, PrimaryWindow, WindowResolution},
};
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use bevy_fmod_simple::*;
use std::time::Duration;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Demo".to_string(),
                    resolution: WindowResolution::new(1280., 720.),
                    position: WindowPosition::Centered(MonitorSelection::Primary),
                    ..default()
                }),
                ..default()
            }),
            EguiPlugin,
            FmodAudioPlugin::default(),
        ))
        .init_resource::<MouselookEnabled>()
        .add_systems(Startup, (load_assets, start_music))
        .add_systems(
            Update,
            (
                spawn_scene.run_if(not(resource_exists::<SceneSpawned>)),
                move_player,
                rotate_player,
                follow_path,
                generate_footsteps,
                toggle_music,
                draw_menu,
                exit_app_on_key,
            ),
        )
        .run()
}

#[derive(Resource)]
struct SceneSpawned;

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<SoundAssets>,
    asset_server: Res<AssetServer>,
) {
    // assets must be loaded before spawning
    if !assets.loaded(&asset_server) {
        return;
    }
    commands.insert_resource(SceneSpawned);

    // player
    let player_height = 1.6;
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_translation(Vec3::new(0., player_height, 0.)),
            tonemapping: Tonemapping::None, // so bevy's luts feature is not required
            ..default()
        },
        AudioListener,
        Footsteps {
            offset: Vec3::NEG_Y * (player_height - 0.1), // at the feet, slightly above the floor
            ..default()
        },
    ));

    // ground
    commands.spawn(PbrBundle {
        mesh: meshes.add(PlaneMeshBuilder {
            plane: Plane3d::new(Vec3::Y),
            half_size: Vec2::ONE * 100.,
        }),
        material: materials.add(StandardMaterial {
            base_color: Color::DARK_GRAY,
            reflectance: 0.7,
            ..default()
        }),
        ..default()
    });

    // light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            color: Color::WHITE,
            intensity: 2_000.,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_translation(Vec3::new(10., 3., -8.)),
        ..default()
    });

    // cube pbr bundle
    let mut cube = |pos, color: Color| PbrBundle {
        mesh: meshes.add(Mesh::from(Cuboid::from_size(Vec3::splat(1.)))),
        material: materials.add(StandardMaterial {
            base_color: color,
            emissive: color * 0.25,
            ..default()
        }),
        transform: Transform::from_translation(pos),
        ..default()
    };

    // moving engine blocks
    for (color, speed, points) in [
        // fast boi
        (
            Color::ORANGE_RED,
            15.,
            vec![
                Vec3::new(3., 1.3, -10.),
                Vec3::new(-2., 1.3, -40.),
                Vec3::new(40., 3., -10.),
            ],
        ),
        // static boi
        (Color::BLACK, 0., vec![Vec3::new(-6., 1., -10.)]),
        // up-down boi
        (
            Color::RED,
            10.,
            vec![Vec3::new(3., -20., 40.), Vec3::new(3., 20., 40.)],
        ),
        // two static bois to check vertical difference
        (Color::GREEN, 0., vec![Vec3::new(-40., -3., -1.)]),
        (
            Color::GREEN,
            0.,
            vec![Vec3::new(-40., 3. + player_height, 1.)],
        ),
    ] {
        commands.spawn((
            cube(points[0], color),
            FollowPath {
                points,
                speed,
                index: 0,
            },
            //
            assets.engine.clone(),
            AudioLoop,
            AudioParameters {
                volume: 0.7,
                min_distance: 1.,
                max_distance: 30.,
                ..default()
            }
            .get_randomized(),
        ));
    }

    commands.spawn(cube(Vec3::new(3., 0., 40.), Color::CRIMSON));
    commands.spawn(cube(Vec3::new(-40., 0., -1.), Color::DARK_GREEN));
}

#[derive(Resource)]
struct SoundAssets {
    footsteps: Vec<Handle<AudioSource>>,
    engine: Handle<AudioSource>,
}

impl SoundAssets {
    fn loaded(&self, server: &AssetServer) -> bool {
        let loaded = |id| match server.get_load_state(id) {
            Some(LoadState::Loaded) => true,
            _ => false,
        };
        self.footsteps.iter().all(|h| loaded(h.id())) && loaded(self.engine.id())
    }
}

fn load_assets(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(SoundAssets {
        footsteps: vec![server.load("Concrete 1.ogg"), server.load("Concrete 2.ogg")],
        engine: server.load("objamb_conv.ogg"),
    })
}

fn move_player(
    mut player: Query<&mut Transform, With<AudioListener>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let regular_speed = 6.;
    let fast_speed = 12.;

    for mut transform in player.iter_mut() {
        let mut move_dir = Vec3::ZERO;
        if keys.pressed(KeyCode::KeyW) {
            move_dir.z -= 1.;
        }
        if keys.pressed(KeyCode::KeyS) {
            move_dir.z += 1.;
        }
        if keys.pressed(KeyCode::KeyA) {
            move_dir.x -= 1.;
        }
        if keys.pressed(KeyCode::KeyD) {
            move_dir.x += 1.;
        }

        let speed = match keys.pressed(KeyCode::ShiftLeft) {
            false => regular_speed,
            true => fast_speed,
        };

        // move on horizontal plane only, regardless of rotation
        let rotation = transform.rotation.to_euler(EulerRot::YXZ).0;
        transform.translation += Quat::from_rotation_y(rotation)
            * move_dir.normalize_or_zero()
            * speed
            * time.delta_seconds();
    }
}

#[derive(Resource, Default)]
struct MouselookEnabled(bool);

fn rotate_player(
    mut player: Query<&mut Transform, With<AudioListener>>,
    mut motion: EventReader<MouseMotion>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut mouselook_enabled: ResMut<MouselookEnabled>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = window.get_single_mut() else {
        return;
    };

    let mouse_sensitivity = 120_f32.to_radians() / window.width();
    let key_sensitivity = 120_f32.to_radians() * time.delta_seconds();

    for mut transform in player.iter_mut() {
        if keys.just_pressed(KeyCode::Space) {
            mouselook_enabled.0 = !mouselook_enabled.0;
            match mouselook_enabled.0 {
                true => {
                    window.cursor.grab_mode = CursorGrabMode::Locked;
                    window.cursor.visible = false;
                }
                false => {
                    window.cursor.grab_mode = CursorGrabMode::None;
                    window.cursor.visible = true;
                }
            }
        }

        let mut rotate = match mouselook_enabled.0 {
            true => {
                let delta = motion
                    .read()
                    .fold(Vec2::ZERO, |sum, event| sum + event.delta);
                delta * mouse_sensitivity
            }
            false => Vec2::ZERO,
        };
        if keys.pressed(KeyCode::ArrowLeft) {
            rotate.x -= key_sensitivity;
        }
        if keys.pressed(KeyCode::ArrowRight) {
            rotate.x += key_sensitivity;
        }
        if keys.pressed(KeyCode::ArrowUp) {
            rotate.y -= key_sensitivity;
        }
        if keys.pressed(KeyCode::ArrowDown) {
            rotate.y += key_sensitivity;
        }

        let old_rotation = transform.rotation;
        transform.rotation = new_camera_rotation(old_rotation, rotate);
    }
}

fn new_camera_rotation(old_rotation: Quat, angles: Vec2) -> Quat {
    let yaw = Quat::from_axis_angle(Vec3::Y, -angles.x);
    let pitch = Quat::from_axis_angle(Vec3::X, -angles.y);

    let rotation = (yaw * old_rotation) * pitch;

    if (rotation * Vec3::Y).y > 0. {
        rotation
    } else {
        yaw * old_rotation
    }
}

/// Entity will follow specified path in loop
#[derive(Component)]
struct FollowPath {
    points: Vec<Vec3>,
    speed: f32,
    index: usize,
}

fn follow_path(mut entities: Query<(&mut Transform, &mut FollowPath)>, time: Res<Time>) {
    for (mut transform, mut path) in entities.iter_mut() {
        let target = path.points[path.index];
        let delta = target - transform.translation;

        let speed = path.speed * time.delta_seconds();

        if delta.length() < speed {
            transform.translation = target;
            path.index = (path.index + 1) % path.points.len();
        } else {
            transform.translation += delta.normalize_or_zero() * speed;
        }
    }
}

/// Spawn footstep sounds for this entity.
///
/// Sounds are made after moving some distance when walking and each time after
/// stopping moving.
#[derive(Component, Default)]
struct Footsteps {
    /// Config - sound origin relative to the entity
    offset: Vec3,

    last_pos: Vec3,      // position of last sound made
    last_time: Duration, // time of last sound made

    prev_pos: Vec3, // entity position on previous frame
    walking: bool,  // entity was moving last frame
}

fn generate_footsteps(
    mut footsteps: Query<(&GlobalTransform, &mut Footsteps)>,
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SoundAssets>,
) {
    let step_distance = 3.; // minimal distance entity should move to make footstep sound
    let step_cooldown = Duration::from_secs_f32(0.1); // don't make sounds more often than this

    for (transform, mut steps) in footsteps.iter_mut() {
        let pos = transform.translation() + steps.offset;

        let speed = pos.distance(steps.prev_pos) / time.delta_seconds();
        steps.prev_pos = pos;

        let make_sound = if speed > 1. {
            steps.walking = true;
            pos.distance(steps.last_pos) > step_distance
        } else {
            std::mem::take(&mut steps.walking)
        };

        let since_last = time.elapsed().saturating_sub(steps.last_time);
        if make_sound && since_last >= step_cooldown {
            steps.last_pos = pos;
            steps.last_time = time.elapsed();

            // pseudorandom value
            let index = time.elapsed().as_millis() as usize % assets.footsteps.len();

            commands.spawn((
                SpatialBundle::from_transform(Transform::from_translation(pos)),
                assets.footsteps.get(index).unwrap().clone(),
                AudioParameters {
                    volume: 0.5,
                    min_distance: 15., // don't pan sound when it's too close to the listener
                    ..default()
                }
                .get_randomized(),
            ));
        }
    }
}

#[derive(Component)]
struct Music {
    enabled: bool,
}

impl Music {
    const SOURCE_VOLUME: f32 = 0.5;
}

fn start_music(mut commands: Commands, mut assets: ResMut<Assets<AudioSource>>) {
    let mut source =
        AudioSource::stream_file("assets/The_Absence_Of_Time.ogg".to_string()).unwrap();
    source.params.volume = Music::SOURCE_VOLUME;

    let asset = assets.add(source);
    commands.spawn((
        asset,
        MUSIC_GROUP,
        AudioLoop,
        AudioParameters {
            volume: Music::SOURCE_VOLUME,
            ..default()
        },
        Music { enabled: true },
    ));
}

fn toggle_music(
    mut music: Query<(&mut AudioParameters, &mut Music)>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        if let Ok((mut params, mut music)) = music.get_single_mut() {
            // TODO: add pause option

            music.enabled = !music.enabled;
            params.volume = match music.enabled {
                true => Music::SOURCE_VOLUME,
                false => 0.,
            };
        }
    }
}

const SFX_GROUP: AudioGroup = AudioGroup(0); // default one
const MUSIC_GROUP: AudioGroup = AudioGroup(1);

fn draw_menu(
    mut egui_ctx: EguiContexts,
    mouselook_enabled: Res<MouselookEnabled>,
    mut settings: ResMut<AudioSettings>,
) {
    egui::Area::new("demo menu".into())
        .anchor(egui::Align2::LEFT_TOP, egui::Vec2::ZERO)
        .show(egui_ctx.ctx_mut(), |ui| {
            ui.heading("bevy_fmod_simple demo");

            ui.label("");

            ui.heading("Controls: ");
            ui.label("- [W/A/S/D] - move");
            ui.label("- [arrows] - look around");
            ui.label("- [Space] - toggle mouselook (off by default)");
            ui.label("- [M] - toggle music (on by default)");
            ui.label("- [ESC] or [Ctrl + Q] - exit");

            ui.label("");

            ui.heading("Sounds: ");
            ui.label("- player's own footsteps;");
            ui.label("- humming cubes;");
            ui.label("- music.");

            ui.label("");

            if mouselook_enabled.0 {
                ui.label("Menu is hidden with mouselook on");
            } else {
                let mut volume_slider = |text: &str, value: &mut f32| {
                    ui.add(egui::Slider::new(value, 0. ..=1.).text(text));
                };

                // yes, these are linear

                volume_slider("Master volume", &mut settings.master_volume);
                volume_slider(
                    "Effects volume",
                    &mut settings.groups.entry(SFX_GROUP).or_default().volume,
                );
                volume_slider(
                    "Music volume",
                    &mut settings.groups.entry(MUSIC_GROUP).or_default().volume,
                );

                ui.add(
                    egui::Slider::new(&mut settings.engine.doppler_scale, 0. ..=2.)
                        .show_value(true)
                        .text("Doppler effect strength"),
                );
            }
        });
}

fn exit_app_on_key(keys: Res<ButtonInput<KeyCode>>, mut exit: EventWriter<AppExit>) {
    if keys.pressed(KeyCode::Escape) || keys.all_pressed([KeyCode::ControlLeft, KeyCode::KeyQ]) {
        exit.send_default();
    }
}
