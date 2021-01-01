use agarlib::*;
use bevy::{app::ScheduleRunnerSettings, prelude::*};
use bevy_networking_turbulence::NetworkResource;
use rand::Rng;
use std::time::Duration;

fn main() {
    App::build().add_plugin(AgarSrv).run();
}

struct AgarSrv;

impl Plugin for AgarSrv {
    fn build(&self, app: &mut AppBuilder) {
        tracing_subscriber::fmt().init();

        app.add_resource(ScheduleRunnerSettings::run_loop(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .add_plugins(MinimalPlugins)
        .add_system(movement_system.system())
        .add_resource(NetworkBroadcast { frame: 0 })
        .add_system_to_stage(stage::PRE_UPDATE, handle_messages.system())
        .add_system_to_stage(stage::POST_UPDATE, network_broadcast_system.system())
        .add_plugin(NetworkPlugin { server: true });
    }
}

fn movement_system(time: Res<Time>, mut agars: Query<(&Agar, &mut Transform)>) {
    for (agar, mut transform) in agars.iter_mut() {
        let vel = input_to_velocity(&agar.velocity, agar.max_velocity);
        transform.translation = transform.translation + (vel * time.delta_seconds());
        transform.translation.x = transform.translation.x.max(0.0).min(WORLD_WIDTH);
        transform.translation.y = transform.translation.y.max(0.0).min(WORLD_HEIGHT);
    }
}

fn network_broadcast_system(
    mut state: ResMut<NetworkBroadcast>,
    mut net: ResMut<NetworkResource>,
    agars: Query<(Entity, &Agar, &Transform)>,
) {
    let message = GameStateMessage {
        frame: state.frame,
        agars: agars
            .iter()
            .map(|(entity, agar, transform)| {
                (
                    entity.id(),
                    Update {
                        agar: agar.clone(),
                        translation: transform.translation,
                    },
                )
            })
            .collect(),
    };
    state.frame += 1;

    net.broadcast_message(message);
}

fn handle_messages(
    commands: &mut Commands,
    mut net: ResMut<NetworkResource>,
    mut balls: Query<(&mut Agar, &NetworkHandle)>,
) {
    let mut acks = vec![];

    for (handle, connection) in net.connections.iter_mut() {
        let channels = connection.channels().unwrap();

        while let Some(client_message) = channels.recv::<ClientMessage>() {
            debug!(
                "ClientMessage received on [{}]: {:?}",
                handle, client_message
            );
            match client_message {
                ClientMessage::Login => {
                    let mut rng = rand::thread_rng();
                    let vel_x = rng.gen_range(-0.5..=0.5);
                    let vel_y = rng.gen_range(-0.5..=0.5);
                    let pos_x = rng.gen_range(0.0..WORLD_WIDTH);
                    let pos_y = rng.gen_range(0.0..WORLD_HEIGHT);
                    info!("Spawning {}x{} {}/{}", pos_x, pos_y, vel_x, vel_y);

                    let entity = commands
                        .spawn((
                            Agar::new(),
                            NetworkHandle::new(*handle),
                            Transform::from_translation(Vec3::new(pos_x, pos_y, 1.0)),
                        ))
                        .current_entity()
                        .unwrap();

                    acks.push((*handle, entity.id()));
                }
                ClientMessage::Input(vel) => {
                    for (mut agar, hd) in balls.iter_mut() {
                        if hd.id == *handle {
                            agar.velocity = vel;
                        }
                    }
                }
                _ => {}
            }
        }

        while let Some(_state_message) = channels.recv::<GameStateMessage>() {
            error!("GameStateMessage received on [{}]", handle);
        }
    }

    for (handle, id) in acks {
        info!("Send ack to {}", id);

        match net.send_message(handle, ClientMessage::LoginAck(id)) {
            Ok(Some(msg)) => error!("unable to send login message: {:?}", msg),
            Err(err) => error!("unable to send login message: {}", err),
            _ => {}
        }
    }
}
