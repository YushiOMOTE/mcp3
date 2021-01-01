use agarlib::*;
use bevy::{prelude::*, render::camera::WindowOrigin};
use bevy_networking_turbulence::{NetworkEvent, NetworkResource};
use bevy_prototype_lyon::prelude::*;
use rand::Rng;
use std::collections::HashMap;

fn main() {
    App::build().add_plugin(BallsExample).run();
}

struct BallsExample;

impl Plugin for BallsExample {
    fn build(&self, app: &mut AppBuilder) {
        app.add_resource(WindowDescriptor {
            width: BOARD_WIDTH as f32,
            height: BOARD_HEIGHT as f32,
            ..Default::default()
        })
        .add_plugins(bevy_webgl2::DefaultPlugins)
        .add_resource(ClearColor(Color::rgb(0.3, 0.3, 0.3)))
        .add_startup_system(client_setup.system())
        .add_system_to_stage(stage::PRE_UPDATE, handle_messages_client.system())
        .add_resource(ServerIds::default())
        .add_system(ball_control_system.system())
        .add_plugin(NetworkPlugin { server: false })
        .add_system(handle_packets.system());
    }
}

fn ball_control_system(mut net: ResMut<NetworkResource>, keyboard_input: Res<Input<KeyCode>>) {
    if keyboard_input.pressed(KeyCode::Left) {
        net.broadcast_message(ClientMessage::Direction(agarlib::Direction::Left));
    }

    if keyboard_input.pressed(KeyCode::Right) {
        net.broadcast_message(ClientMessage::Direction(agarlib::Direction::Right));
    }
}

fn client_setup(commands: &mut Commands) {
    let mut camera = Camera2dBundle::default();
    camera.orthographic_projection.window_origin = WindowOrigin::BottomLeft;
    commands.spawn(camera);
}

fn handle_packets(
    commands: &mut Commands,
    mut net: ResMut<NetworkResource>,
    mut state: ResMut<NetworkReader>,
    network_events: Res<Events<NetworkEvent>>,
) {
    for event in state.network_events.iter(&network_events) {
        match event {
            NetworkEvent::Connected(handle) => match net.connections.get_mut(handle) {
                Some(connection) => {
                    match connection.remote_address() {
                        Some(remote_address) => {
                            debug!(
                                "Incoming connection on [{}] from [{}]",
                                handle, remote_address
                            );

                            // New client connected - spawn a ball
                            let mut rng = rand::thread_rng();
                            let vel_x = rng.gen_range(-0.5..=0.5);
                            let vel_y = rng.gen_range(-0.5..=0.5);
                            let pos_x = rng.gen_range(0..BOARD_WIDTH) as f32;
                            let pos_y = rng.gen_range(0..BOARD_HEIGHT) as f32;
                            info!("Spawning {}x{} {}/{}", pos_x, pos_y, vel_x, vel_y);
                            commands.spawn((
                                Ball {
                                    velocity: 400.0 * Vec3::new(vel_x, vel_y, 0.0).normalize(),
                                },
                                Pawn {
                                    controller: *handle,
                                },
                                Transform::from_translation(Vec3::new(pos_x, pos_y, 1.0)),
                            ));
                        }
                        None => {
                            debug!("Connected on [{}]", handle);
                        }
                    }

                    debug!("Sending Hello on [{}]", handle);
                    match net.send_message(*handle, ClientMessage::Hello("test".to_string())) {
                        Ok(msg) => match msg {
                            Some(msg) => {
                                error!("Unable to send Hello: {:?}", msg);
                            }
                            None => {}
                        },
                        Err(err) => {
                            error!("Unable to send Hello: {:?}", err);
                        }
                    };
                }
                None => panic!("Got packet for non-existing connection [{}]", handle),
            },
            _ => {}
        }
    }
}

type ServerIds = HashMap<u32, (u32, u32)>;

fn handle_messages_client(
    commands: &mut Commands,
    mut net: ResMut<NetworkResource>,
    mut server_ids: ResMut<ServerIds>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut balls: Query<(Entity, &mut Ball, &mut Transform)>,
) {
    for (handle, connection) in net.connections.iter_mut() {
        let channels = connection.channels().unwrap();
        while let Some(_client_message) = channels.recv::<ClientMessage>() {
            error!("ClientMessage received on [{}]", handle);
        }

        // it is possible that many state updates came at the same time - spawn once
        let mut to_spawn: HashMap<u32, (u32, Vec3, Vec3)> = HashMap::new();

        while let Some(mut state_message) = channels.recv::<GameStateMessage>() {
            let message_frame = state_message.frame;
            info!(
                "GameStateMessage received on [{}]: {:?}",
                handle, state_message
            );

            // update all balls
            for (entity, mut ball, mut transform) in balls.iter_mut() {
                let server_id_entry = server_ids.get_mut(&entity.id()).unwrap();
                let (server_id, update_frame) = *server_id_entry;

                if let Some(index) = state_message
                    .balls
                    .iter()
                    .position(|&update| update.0 == server_id)
                {
                    let (_id, velocity, translation) = state_message.balls.remove(index);

                    if update_frame > message_frame {
                        continue;
                    }
                    server_id_entry.1 = message_frame;

                    ball.velocity = velocity;
                    transform.translation = translation;
                } else {
                    // TODO: despawn disconnected balls
                }
            }
            // create new balls
            for (id, velocity, translation) in state_message.balls.drain(..) {
                if let Some((frame, _velocity, _translation)) = to_spawn.get(&id) {
                    if *frame > message_frame {
                        continue;
                    }
                };
                to_spawn.insert(id, (message_frame, velocity, translation));
            }
        }

        for (id, (frame, velocity, translation)) in to_spawn.iter() {
            info!("Spawning {} @{}", id, frame);
            let material = materials.add(Color::rgb(0.8, 0.0, 0.0).into());

            let entity = commands
                .spawn(primitive(
                    material.clone(),
                    &mut meshes,
                    ShapeType::Circle(15.0),
                    TessellationMode::Fill(&FillOptions::default()),
                    Vec3::new(0.0, 0.0, 0.0).into(),
                ))
                .with(Ball {
                    velocity: *velocity,
                })
                .with(Pawn { controller: *id })
                .current_entity()
                .unwrap();
            server_ids.insert(entity.id(), (*id, *frame));
        }
    }
}
