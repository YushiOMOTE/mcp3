use agarlib::*;
use bevy::{app::ScheduleRunnerSettings, prelude::*};
use bevy_networking_turbulence::{NetworkEvent, NetworkResource};
use rand::Rng;
use std::time::Duration;

fn main() {
    App::build().add_plugin(BallsExample).run();
}

struct BallsExample;

impl Plugin for BallsExample {
    fn build(&self, app: &mut AppBuilder) {
        tracing_subscriber::fmt().init();

        app.add_resource(ScheduleRunnerSettings::run_loop(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .add_plugins(MinimalPlugins)
        .add_system(ball_movement_system.system())
        .add_resource(NetworkBroadcast { frame: 0 })
        .add_system_to_stage(stage::PRE_UPDATE, handle_messages_server.system())
        .add_system_to_stage(stage::POST_UPDATE, network_broadcast_system.system())
        .add_plugin(NetworkPlugin { server: true })
        .add_system(handle_packets.system());
    }
}

fn ball_movement_system(time: Res<Time>, mut ball_query: Query<(&Ball, &mut Transform)>) {
    for (ball, mut transform) in ball_query.iter_mut() {
        let mut translation = transform.translation + (ball.velocity * time.delta_seconds());
        let mut x = translation.x as i32 % BOARD_WIDTH as i32;
        let mut y = translation.y as i32 % BOARD_HEIGHT as i32;
        if x < 0 {
            x += BOARD_WIDTH as i32;
        }
        if y < 0 {
            y += BOARD_HEIGHT as i32;
        }
        translation.x = x as f32;
        translation.y = y as f32;
        transform.translation = translation;
    }
}

fn network_broadcast_system(
    mut state: ResMut<NetworkBroadcast>,
    mut net: ResMut<NetworkResource>,
    ball_query: Query<(Entity, &Ball, &Transform)>,
) {
    let mut message = GameStateMessage {
        frame: state.frame,
        balls: Vec::new(),
    };
    state.frame += 1;

    for (entity, ball, transform) in ball_query.iter() {
        message
            .balls
            .push((entity.id(), ball.velocity, transform.translation));
    }

    net.broadcast_message(message);
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
                }
                None => panic!("Got packet for non-existing connection [{}]", handle),
            },
            e => {
                info!("{:?}", e)
            }
        }
    }
}

fn handle_messages_server(mut net: ResMut<NetworkResource>, mut balls: Query<(&mut Ball, &Pawn)>) {
    for (handle, connection) in net.connections.iter_mut() {
        let channels = connection.channels().unwrap();
        while let Some(client_message) = channels.recv::<ClientMessage>() {
            debug!(
                "ClientMessage received on [{}]: {:?}",
                handle, client_message
            );
            match client_message {
                ClientMessage::Hello(id) => {
                    info!("Client [{}] connected on [{}]", id, handle);
                    // TODO: store client id?
                }
                ClientMessage::Direction(dir) => {
                    let mut angle: f32 = 0.03;
                    if dir == agarlib::Direction::Right {
                        angle *= -1.0;
                    }
                    for (mut ball, pawn) in balls.iter_mut() {
                        if pawn.controller == *handle {
                            ball.velocity = Quat::from_rotation_z(angle) * ball.velocity;
                        }
                    }
                }
            }
        }

        while let Some(_state_message) = channels.recv::<GameStateMessage>() {
            error!("GameStateMessage received on [{}]", handle);
        }
    }
}
