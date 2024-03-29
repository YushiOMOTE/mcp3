use agarlib::*;
use bevy::{app::ScheduleRunnerSettings, prelude::*};
use bevy_networking_turbulence::NetworkResource;
use rand::Rng;
use std::{collections::HashMap, time::Duration};

fn main() {
    App::build().add_plugin(AgarSrv).run();
}

#[derive(Default)]
struct FeedUpdates {
    updates: Vec<FeedUpdate>,
    snapshot: HashMap<EntityId, FeedUpdateSpawn>,
    total_feeds: usize,
}

impl FeedUpdates {
    fn spawn(&mut self, update: FeedUpdateSpawn) {
        self.updates.push(FeedUpdate::Spawn(update.clone()));
        self.snapshot.insert(update.id, update);
        self.total_feeds += 1;
    }

    fn despawn(&mut self, id: EntityId) {
        self.updates.push(FeedUpdate::Despawn(id));
        self.snapshot.remove(&id);
        self.total_feeds -= 1;
    }

    fn snapshot(&self) -> Vec<FeedUpdate> {
        self.snapshot
            .iter()
            .map(|(_, update)| FeedUpdate::Spawn(update.clone()))
            .collect()
    }

    fn updates(&self, from: usize) -> Vec<FeedUpdate> {
        let from = from.min(self.updates.len());
        let mut updates = HashMap::new();

        for update in &self.updates[from..] {
            match update {
                FeedUpdate::Spawn(s) => {
                    updates.insert(s.id, update.clone());
                }
                FeedUpdate::Despawn(id) => {
                    if updates.remove(id).is_none() {
                        updates.insert(*id, update.clone());
                    }
                }
            }
        }

        updates.into_iter().map(|(_, update)| update).collect()
    }
}

struct AgarSrv;

impl Plugin for AgarSrv {
    fn build(&self, app: &mut AppBuilder) {
        tracing_subscriber::fmt().init();

        app.add_resource(ScheduleRunnerSettings::run_loop(Duration::from_secs_f64(
            1.0 / 30.0,
        )))
        .add_resource(FeedUpdates::default())
        .add_plugins(MinimalPlugins)
        .add_system(movement_system.system())
        .add_resource(NetworkBroadcast { frame: 0 })
        .add_system_to_stage(stage::PRE_UPDATE, handle_messages.system())
        .add_system(feed_collision_system.system())
        .add_system(feed_spawn_system.system())
        .add_system_to_stage(stage::POST_UPDATE, network_broadcast_system.system())
        .add_plugin(NetworkPlugin { server: true });
    }
}

fn feed_spawn_system(commands: &mut Commands, mut feed_updates: ResMut<FeedUpdates>) {
    while feed_updates.total_feeds < 100 {
        info!("Spawn feed {}", feed_updates.total_feeds);

        let mut rng = rand::thread_rng();
        let pos_x = rng.gen_range(0.0..WORLD_WIDTH);
        let pos_y = rng.gen_range(0.0..WORLD_HEIGHT);

        let color = FeedColor::Blue;
        let transform = Transform::from_translation(Vec3::new(pos_x, pos_y, 0.0));

        let entity = commands
            .spawn((Feed { color }, transform.clone()))
            .current_entity()
            .unwrap();

        feed_updates.spawn(FeedUpdateSpawn {
            id: entity.id(),
            color,
            translation: transform.translation.clone(),
        });
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
    feed_updates: Res<FeedUpdates>,
) {
    let message = GameStateMessage {
        frame: state.frame,
        agars: agars
            .iter()
            .map(|(entity, agar, transform)| {
                (
                    entity.id(),
                    AgarUpdate {
                        agar: agar.clone(),
                        translation: transform.translation,
                    },
                )
            })
            .collect(),
        feeds: feed_updates.updates.len() as u64,
    };
    state.frame += 1;

    net.broadcast_message(message);
}

fn feed_collision_system(
    commands: &mut Commands,
    mut feed_updates: ResMut<FeedUpdates>,
    mut agars: Query<(Entity, &mut Agar, &Transform)>,
    feeds: Query<(Entity, &Feed, &Transform)>,
) {
    for (_entity, mut agar, agar_transform) in agars.iter_mut() {
        for (entity, _feed, feed_transform) in feeds.iter() {
            let p = agar_transform.translation;
            let q = feed_transform.translation;
            if p.distance(q) < agar.size {
                info!("Despawn feed");
                feed_updates.despawn(entity.id());
                commands.despawn(entity);
                agar.grow(1.0);
            }
        }
    }
}

fn handle_messages(
    commands: &mut Commands,
    mut net: ResMut<NetworkResource>,
    mut balls: Query<(&mut Agar, &NetworkHandle)>,
    feed_updates: Res<FeedUpdates>,
) {
    let mut acks = vec![];
    let mut feeds = vec![];

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
                ClientMessage::FeedRequest(update_id) => {
                    let updates = if update_id == 0 {
                        feed_updates.snapshot()
                    } else {
                        feed_updates.updates(update_id as usize)
                    };

                    feeds.push((*handle, updates.to_vec()));
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

    for (handle, feeds) in feeds {
        info!("Send feeds to client {}", handle);

        match net.send_message(handle, ClientMessage::FeedResponse(feeds)) {
            Ok(Some(msg)) => error!("unable to send feeds to client: {:?}", msg),
            Err(err) => error!("unable to send feeds to client: {}", err),
            _ => {}
        }
    }
}
