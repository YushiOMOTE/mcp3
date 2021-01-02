use bevy::prelude::*;
use bevy_networking_turbulence::{
    ConnectionChannelsBuilder, MessageChannelMode, MessageChannelSettings, NetworkEvent,
    NetworkResource, NetworkingPlugin, ReliableChannelSettings,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, time::Duration};

const SERVER_PORT: u16 = 14192;

pub const AGAR_INIT_RADIUS: f32 = 10.0;
pub const AGAR_MAX_RADIUS: f32 = 1000.0;

pub fn max_velocity(radius: f32) -> f32 {
    1000.0 / (radius + 1.0 - AGAR_INIT_RADIUS)
}

pub const WINDOW_WIDTH: f32 = 1000.0;
pub const WINDOW_HEIGHT: f32 = 1000.0;

pub const WORLD_WIDTH: f32 = 2000.0;
pub const WORLD_HEIGHT: f32 = 2000.0;

pub fn input_to_velocity(pos: &Vec2, max: f32) -> Vec3 {
    let w = 0.5;
    let x = (pos.x - WINDOW_WIDTH / 2.0) * w;
    let y = (pos.y - WINDOW_HEIGHT / 2.0) * w;
    let l = (x.powf(2.0) + y.powf(2.0)).sqrt();
    let w = l.min(max) / l;

    Vec3::new(x, y, 0.0) * w
}

pub type EntityId = u32;

#[derive(Default)]
pub struct NetworkBroadcast {
    pub frame: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Update {
    pub agar: Agar,
    pub translation: Vec3,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameStateMessage {
    pub frame: u32,
    pub agars: HashMap<EntityId, Update>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    Login,
    LoginAck(EntityId),
    Input(Vec2),
}

#[derive(Debug)]
pub struct NetworkHandle {
    pub id: u32,
}

impl NetworkHandle {
    pub fn new(id: u32) -> Self {
        Self { id }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpdateContext {
    pub id: EntityId,
    pub frame: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agar {
    pub radius: f32,
    pub velocity: Vec2,
    pub max_velocity: f32,
}

impl Agar {
    pub fn new() -> Self {
        Self {
            radius: AGAR_INIT_RADIUS,
            velocity: Vec2::zero(),
            max_velocity: max_velocity(AGAR_INIT_RADIUS),
        }
    }
}

pub struct NetworkPlugin {
    pub server: bool,
}

const CLIENT_STATE_MESSAGE_SETTINGS: MessageChannelSettings = MessageChannelSettings {
    channel: 0,
    channel_mode: MessageChannelMode::Reliable {
        reliability_settings: ReliableChannelSettings {
            bandwidth: 4096,
            recv_window_size: 1024,
            send_window_size: 1024,
            burst_bandwidth: 1024,
            init_send: 512,
            wakeup_time: Duration::from_millis(100),
            initial_rtt: Duration::from_millis(200),
            max_rtt: Duration::from_secs(2),
            rtt_update_factor: 0.1,
            rtt_resend_factor: 1.5,
        },
        max_message_len: 1024,
    },
    message_buffer_size: 8,
    packet_buffer_size: 8,
};

const GAME_STATE_MESSAGE_SETTINGS: MessageChannelSettings = MessageChannelSettings {
    channel: 1,
    channel_mode: MessageChannelMode::Unreliable,
    message_buffer_size: 8,
    packet_buffer_size: 8,
};

#[derive(Default)]
pub struct NetworkReader {
    pub network_events: EventReader<NetworkEvent>,
}

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut AppBuilder) {
        if self.server {
            #[cfg(not(target_arch = "wasm32"))]
            {
                app.add_startup_system(server_setup.system())
            }
            #[cfg(target_arch = "wasm32")]
            {
                app
            }
        } else {
            app.add_startup_system(client_setup.system())
        }
        .add_plugin(NetworkingPlugin)
        .add_startup_system(network_setup.system())
        .add_resource(NetworkReader::default());
    }
}

fn network_setup(mut net: ResMut<NetworkResource>) {
    net.set_channels_builder(|builder: &mut ConnectionChannelsBuilder| {
        builder
            .register::<ClientMessage>(CLIENT_STATE_MESSAGE_SETTINGS)
            .unwrap();
        builder
            .register::<GameStateMessage>(GAME_STATE_MESSAGE_SETTINGS)
            .unwrap();
    });
}

fn client_setup(mut net: ResMut<NetworkResource>) {
    let socket_address = SocketAddr::new("172.23.76.35".parse().unwrap(), SERVER_PORT);
    info!("Starting client");
    net.connect(socket_address);
}

#[cfg(not(target_arch = "wasm32"))]
fn server_setup(mut net: ResMut<NetworkResource>) {
    let socket_address = SocketAddr::new("172.23.76.35".parse().unwrap(), SERVER_PORT);
    info!("Starting server: {}", socket_address);
    net.listen(socket_address);
}
