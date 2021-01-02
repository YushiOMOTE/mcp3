use agarlib::*;
use bevy::{prelude::*, render::camera::Camera};
use bevy_networking_turbulence::{NetworkEvent, NetworkResource};
use bevy_prototype_lyon::prelude::*;
use rand::Rng;
use std::collections::HashMap;

fn main() {
    App::build().add_plugin(AgarCli).run();
}

#[derive(Default)]
struct PlayerInfo {
    id: Option<EntityId>,
}

struct AgarCli;

impl Plugin for AgarCli {
    fn build(&self, app: &mut AppBuilder) {
        app.add_resource(WindowDescriptor {
            width: WINDOW_WIDTH as f32,
            height: WINDOW_HEIGHT as f32,
            ..Default::default()
        })
        .add_resource(PlayerInfo::default())
        .add_plugins(bevy_webgl2::DefaultPlugins)
        .add_resource(ClearColor(Color::rgb(0.3, 0.3, 0.3)))
        .add_startup_system(camera_setup.system())
        .add_startup_system(background_setup.system())
        .add_system_to_stage(stage::PRE_UPDATE, handle_messages.system())
        .add_system(input_system.system())
        .add_system(camera_system.system())
        .add_system(handle_packets.system())
        .add_plugin(NetworkPlugin { server: false });
    }
}

fn handle_packets(
    mut net: ResMut<NetworkResource>,
    mut state: ResMut<NetworkReader>,
    network_events: Res<Events<NetworkEvent>>,
) {
    for event in state.network_events.iter(&network_events) {
        let handle = match event {
            NetworkEvent::Connected(handle) => handle,
            _ => continue,
        };

        info!("Logging in");
        match net.send_message(*handle, ClientMessage::Login) {
            Ok(Some(msg)) => error!("unable to send login message: {:?}", msg),
            Err(err) => error!("unable to send login message: {}", err),
            _ => {}
        }
    }
}

fn background_setup(
    commands: &mut Commands,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let material = materials.add(Color::rgb(0.0, 1.0, 0.5).into());

    for _ in 0..1000 {
        let mut rng = rand::thread_rng();
        let vel_x = rng.gen_range(-0.5..=0.5);
        let vel_y = rng.gen_range(-0.5..=0.5);
        let pos_x = rng.gen_range(0.0..WORLD_WIDTH);
        let pos_y = rng.gen_range(0.0..WORLD_HEIGHT);
        info!("Spawning {}x{} {}/{}", pos_x, pos_y, vel_x, vel_y);
        commands.spawn(primitive(
            material.clone(),
            &mut meshes,
            ShapeType::Circle(5.0),
            TessellationMode::Fill(&FillOptions::default()),
            Vec3::new(pos_x, pos_y, -100.0),
        ));
    }
}

fn camera_setup(commands: &mut Commands) {
    commands.spawn(Camera2dBundle::default());
}

fn camera_system(
    player: Res<PlayerInfo>,
    mut cameras: Query<(&Camera, &mut Transform)>,
    agars: Query<(&Agar, &UpdateContext, &Transform)>,
) {
    let id = match player.id {
        Some(id) => id,
        None => return,
    };

    for (_camera, mut camera_transform) in cameras.iter_mut() {
        for (_agar, context, transform) in agars.iter() {
            if context.id == id && camera_transform.translation != transform.translation {
                info!("Move camera to {:?}", transform.translation);
                camera_transform.translation = transform.translation.clone();
                break;
            }
        }
    }
}

fn input_system(
    mut net: ResMut<NetworkResource>,
    mut reader: Local<EventReader<CursorMoved>>,
    events: Res<Events<CursorMoved>>,
) {
    for event in reader.iter(&events) {
        net.broadcast_message(ClientMessage::Input(event.position.clone()));
    }
}

fn handle_messages(
    commands: &mut Commands,
    mut net: ResMut<NetworkResource>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut player: ResMut<PlayerInfo>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut agars: Query<(Entity, &mut Agar, &mut UpdateContext, &mut Transform)>,
) {
    for (_, connection) in net.connections.iter_mut() {
        let channels = connection.channels().unwrap();

        while let Some(client_message) = channels.recv::<ClientMessage>() {
            let id = match client_message {
                ClientMessage::LoginAck(id) => id,
                _ => continue,
            };
            player.id = Some(id);
        }

        // to avoid double spawn
        let mut to_spawn = HashMap::new();

        while let Some(mut state_message) = channels.recv::<GameStateMessage>() {
            let message_frame = state_message.frame;

            // update all agars
            for (entity, mut agar, mut context, mut transform) in agars.iter_mut() {
                if let Some(update) = state_message.agars.remove(&context.id) {
                    if context.frame >= message_frame {
                        continue;
                    }
                    context.frame = message_frame;
                    *agar = update.agar;
                    transform.translation = update.translation;
                } else {
                    commands.despawn(entity);
                }
            }

            for (id, update) in state_message.agars.drain() {
                to_spawn.insert(id, (message_frame, update));
            }
        }

        for (id, (message_frame, update)) in to_spawn {
            let material = materials.add(Color::rgb(0.8, 0.0, 0.0).into());
            commands
                .spawn(primitive(
                    material.clone(),
                    &mut meshes,
                    ShapeType::Circle(15.0),
                    TessellationMode::Fill(&FillOptions::default()),
                    update.translation.into(),
                ))
                .with(update.agar.clone())
                .with(UpdateContext {
                    id,
                    frame: message_frame,
                });
        }
    }
}
