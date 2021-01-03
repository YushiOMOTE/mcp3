use agarlib::*;
use bevy::{prelude::*, render::camera::Camera};
use bevy_networking_turbulence::{NetworkEvent, NetworkResource};
use bevy_prototype_lyon::prelude::*;
use std::collections::HashMap;

fn main() {
    App::build().add_plugin(AgarCli).run();
}

#[derive(Default)]
struct PlayerInfo {
    id: Option<EntityId>,
}

#[derive(Default)]
struct FeedState {
    feeds: u64,
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
        .add_resource(FeedState::default())
        .add_plugins(bevy_webgl2::DefaultPlugins)
        .add_resource(ClearColor(Color::rgb(0.3, 0.3, 0.3)))
        .add_startup_system(camera_setup.system())
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
    mut agars: Query<(
        Entity,
        &mut Agar,
        &mut Sprite,
        &mut UpdateContext,
        &mut Transform,
    )>,
    feeds: Query<(Entity, &Feed, &UpdateContext)>,
    mut feed_state: ResMut<FeedState>,
) {
    let mut feed_requests = vec![];

    for (handle, connection) in net.connections.iter_mut() {
        let channels = connection.channels().unwrap();

        let mut feeds_to_despawn = vec![];

        while let Some(client_message) = channels.recv::<ClientMessage>() {
            match client_message {
                ClientMessage::LoginAck(id) => {
                    player.id = Some(id);
                }
                ClientMessage::FeedResponse(updates) => {
                    info!("Receive updates: {:?}", updates);

                    for update in updates {
                        match update {
                            FeedUpdate::Spawn(feed) => {
                                let color = match feed.color {
                                    FeedColor::Red => Color::rgb(0.8, 0.2, 0.2),
                                    FeedColor::Green => Color::rgb(0.2, 0.8, 0.2),
                                    FeedColor::Blue => Color::rgb(0.2, 0.2, 0.8),
                                };

                                let material = materials.add(color.into());

                                commands
                                    .spawn(primitive(
                                        material.clone(),
                                        &mut meshes,
                                        ShapeType::Circle(10.0),
                                        TessellationMode::Fill(&FillOptions::default()),
                                        feed.translation.into(),
                                    ))
                                    .with(Feed { color: feed.color })
                                    .with(UpdateContext {
                                        id: feed.id,
                                        frame: 0,
                                    });
                            }
                            FeedUpdate::Despawn(id) => {
                                feeds_to_despawn.push(id);
                            }
                        }
                    }
                }
                _ => continue,
            }
        }

        // Despawn feeds
        for (entity, _feed, context) in feeds.iter() {
            if feeds_to_despawn.contains(&context.id) {
                commands.despawn(entity);
            }
        }

        // to avoid double spawn
        let mut agars_to_spawn = HashMap::new();
        let mut feed_request_num = None;

        while let Some(mut state_message) = channels.recv::<GameStateMessage>() {
            let message_frame = state_message.frame;

            // update all agars
            for (entity, mut agar, mut sprite, mut context, mut transform) in agars.iter_mut() {
                if let Some(update) = state_message.agars.remove(&context.id) {
                    if context.frame >= message_frame {
                        continue;
                    }
                    context.frame = message_frame;
                    sprite.size.x = update.agar.size * 2.0;
                    sprite.size.y = update.agar.size * 2.0;
                    info!("Agar size: {:?}", sprite.size);
                    *agar = update.agar;
                    transform.translation = update.translation;
                } else {
                    commands.despawn(entity);
                }
            }

            for (id, update) in state_message.agars.drain() {
                agars_to_spawn.insert(id, (message_frame, update));
            }

            if feed_state.feeds < state_message.feeds {
                if feed_request_num.is_none() {
                    feed_request_num = Some(feed_state.feeds);
                }
                feed_state.feeds = state_message.feeds;
            }
        }

        if let Some(num) = feed_request_num {
            feed_requests.push((*handle, num));
        }

        // spawn new agars
        for (id, (message_frame, update)) in agars_to_spawn {
            let material = materials.add(Color::rgb(0.8, 0.0, 0.0).into());
            commands
                .spawn(primitive(
                    material.clone(),
                    &mut meshes,
                    ShapeType::Circle(1.0),
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

    for (handle, num) in feed_requests {
        info!("Requesting feed {}", num);
        match net.send_message(handle, ClientMessage::FeedRequest(num)) {
            Ok(Some(msg)) => error!("unable to send feed request to server: {:?}", msg),
            Err(err) => error!("unable to send feed request to server: {}", err),
            _ => {}
        }
    }
}
