use bevy::prelude::*;
use bevy_networking_turbulence::{
    ConnectionChannelsBuilder, MessageChannelMode, MessageChannelSettings, NetworkEvent,
    NetworkResource, NetworkingPlugin, ReliableChannelSettings,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, time::Duration};

const SERVER_PORT: u16 = 14192;

pub const AGAR_INIT_SIZE: f32 = 15.0;
pub const AGAR_MAX_SIZE: f32 = 500.0;

pub fn max_velocity(size: f32) -> f32 {
    500.0 / ((size - AGAR_INIT_SIZE).powf(0.8) + 1.0) + 50.0
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

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum FeedColor {
    Red,
    Green,
    Blue,
}

pub type EntityId = u32;

#[derive(Default)]
pub struct NetworkBroadcast {
    pub frame: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FeedUpdateSpawn {
    pub id: EntityId,
    pub color: FeedColor,
    pub translation: Vec3,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FeedUpdate {
    Spawn(FeedUpdateSpawn),
    Despawn(EntityId),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgarUpdate {
    pub agar: Agar,
    pub translation: Vec3,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameStateMessage {
    pub frame: u32,
    pub agars: HashMap<EntityId, AgarUpdate>,
    pub feeds: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    Login,
    LoginAck(EntityId),
    Input(Vec2),
    FeedRequest(u64),
    FeedResponse(Vec<FeedUpdate>),
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
pub struct Feed {
    pub color: FeedColor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agar {
    pub size: f32,
    pub velocity: Vec2,
    pub max_velocity: f32,
}

impl Agar {
    pub fn new() -> Self {
        Self {
            size: AGAR_INIT_SIZE,
            velocity: Vec2::zero(),
            max_velocity: max_velocity(AGAR_INIT_SIZE),
        }
    }

    pub fn grow(&mut self, size: f32) {
        self.size += size;
        self.max_velocity = max_velocity(self.size);
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
        max_message_len: 10240,
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

pub const ADDR: Option<&'static str> = option_env!("SERVER_ADDR");

pub fn addr() -> &'static str {
    ADDR.unwrap_or("172.23.76.35")
}

fn client_setup(mut net: ResMut<NetworkResource>) {
    let socket_address = SocketAddr::new(addr().parse().unwrap(), SERVER_PORT);
    info!("Starting client");
    net.connect(socket_address);
}

#[cfg(not(target_arch = "wasm32"))]
fn server_setup(mut net: ResMut<NetworkResource>) {
    let socket_address = SocketAddr::new(addr().parse().unwrap(), SERVER_PORT);
    info!("Starting server: {}", socket_address);
    net.listen(socket_address);
}
