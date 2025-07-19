use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kodama::RECEIVE_BUFFER_SIZE;
use kodama::config::get_config;
use kodama::ipc::zone::ServerZoneIpcSegment;
use kodama::packet::{ConnectionType, PacketState, SegmentData, send_keep_alive};
use kodama::world::ZoneConnection;
use kodama::world::{
    ClientHandle, FromServer, ServerHandle, ToServer, WorldDatabase, handle_custom_ipc,
    server_main_loop,
};

use mlua::Lua;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::join;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, UnboundedReceiver, UnboundedSender, channel, unbounded_channel};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

fn spawn_main_loop() -> (ServerHandle, JoinHandle<()>) {
    let (send, recv) = channel(64);

    let handle = ServerHandle {
        chan: send,
        next_id: Default::default(),
    };

    let join = tokio::spawn(async move {
        let res = server_main_loop(recv).await;
        match res {
            Ok(()) => {}
            Err(err) => {
                tracing::error!("{}", err);
            }
        }
    });

    (handle, join)
}

struct ClientData {
    /// Socket for data recieved from the global server
    recv: Receiver<FromServer>,
    connection: ZoneConnection,
}

/// Spawn a new client actor.
pub fn spawn_client(connection: ZoneConnection) {
    let (send, recv) = channel(64);

    let id = &connection.id.clone();
    let ip = &connection.ip.clone();

    let data = ClientData { recv, connection };

    // Spawn a new client task
    let (my_send, my_recv) = oneshot::channel();
    let _kill = tokio::spawn(start_client(my_recv, data));

    // Send client information to said task
    let handle = ClientHandle {
        id: *id,
        ip: *ip,
        channel: send,
        actor_id: 0,
    };
    let _ = my_send.send(handle);
}

async fn start_client(my_handle: oneshot::Receiver<ClientHandle>, data: ClientData) {
    // Recieve client information from global
    let my_handle = match my_handle.await {
        Ok(my_handle) => my_handle,
        Err(_) => return,
    };

    let connection = data.connection;
    let recv = data.recv;

    // communication channel between client_loop and client_server_loop
    let (internal_send, internal_recv) = unbounded_channel();

    let _ = join!(
        tokio::spawn(client_loop(connection, internal_recv, my_handle)),
        tokio::spawn(client_server_loop(recv, internal_send))
    );
}

async fn client_server_loop(
    mut data: Receiver<FromServer>,
    internal_send: UnboundedSender<FromServer>,
) {
    while let Some(msg) = data.recv().await {
        internal_send.send(msg).unwrap()
    }
}

async fn client_loop(
    mut connection: ZoneConnection,
    mut internal_recv: UnboundedReceiver<FromServer>,
    client_handle: ClientHandle,
) {
    let mut buf = vec![0; RECEIVE_BUFFER_SIZE];
    loop {
        tokio::select! {
            biased; // client data should always be prioritized
            n = connection.socket.read(&mut buf) => {
                match n {
                    Ok(n) => {
                        // if the last response was over >5 seconds, the client is probably gone
                        if n == 0 {
                            let now = Instant::now();
                            if now.duration_since(connection.last_keep_alive) > Duration::from_secs(5) {
                                tracing::info!("Connection {:#?} was killed because of timeout", client_handle.id);
                                break;
                            }
                        }

                        if n > 0 {
                            connection.last_keep_alive = Instant::now();

                            let (segments, _connection_type) = connection.parse_packet(&buf[..n]);
                            for segment in &segments {
                                match &segment.data {
                                    SegmentData::None() => {},
                                    SegmentData::Setup { .. } => todo!(),
                                    SegmentData::Ipc { .. } => todo!(),
                                    SegmentData::KeepAliveRequest { id, timestamp } => {
                                        send_keep_alive::<ServerZoneIpcSegment>(
                                            &mut connection.socket,
                                            &mut connection.state,
                                            ConnectionType::Zone,
                                            *id,
                                            *timestamp,
                                        )
                                        .await
                                    }
                                    SegmentData::KeepAliveResponse { .. } => {
                                        tracing::info!("Got keep alive response from client... cool...");
                                    }
                                    SegmentData::KodamaIpc { data } => handle_custom_ipc(&mut connection, data).await,
                                    _ => {
                                        panic!("The server is recieving a response or unknown packet: {segment:#?}")
                                    }
                                }
                            }
                        }
                    },
                    Err(_) => {
                        tracing::info!("Connection {:#?} was killed because of a network error!", client_handle.id);
                        break;
                    },
                }
            }
            msg = internal_recv.recv() => match msg {
                Some(msg) => match msg {
                    FromServer::Message(_) => todo!(),
                },
                None => break,
            }
        }
    }

    // forcefully log out the player if they weren't logging out but force D/C'd
    if !connection.gracefully_logged_out {
        tracing::info!(
            "Forcefully logging out connection {:#?}...",
            client_handle.id
        );
        connection
            .handle
            .send(ToServer::Disconnected(connection.id))
            .await;
    }
}

async fn handle_rcon(listener: &Option<TcpListener>) -> Option<(TcpStream, SocketAddr)> {
    match listener {
        Some(listener) => Some(listener.accept().await.ok()?),
        None => None,
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = get_config();

    let addr = config.world.get_socketaddr();

    let listener = TcpListener::bind(addr).await.unwrap();

    let rcon_listener = if !config.world.rcon_password.is_empty() {
        Some(
            TcpListener::bind(config.world.get_rcon_socketaddr())
                .await
                .unwrap(),
        )
    } else {
        None
    };

    tracing::info!("Server started on {addr}");

    let database = Arc::new(WorldDatabase::new());
    let lua = Arc::new(Mutex::new(Lua::new()));

    let (handle, _) = spawn_main_loop();

    loop {
        tokio::select! {
            Ok((socket, ip)) = listener.accept() => {
                let id = handle.next_id();

                let state = PacketState {
                    client_key: None,
                };

                spawn_client(ZoneConnection {
                    config: get_config().world,
                    socket,
                    state,
                    ip,
                    id,
                    handle: handle.clone(),
                    database: database.clone(),
                    lua: lua.clone(),
                    last_keep_alive: Instant::now(),
                    gracefully_logged_out: false,
                });
            }
            Some((mut socket, _)) = handle_rcon(&rcon_listener) => {
                let mut authenticated = false;

                loop {
                    // read from client
                    let mut resp_bytes = [0u8; rkon::MAX_PACKET_SIZE];
                    let n = socket.read(&mut resp_bytes).await.unwrap();
                    if n > 0 {
                        let request = rkon::Packet::decode(&resp_bytes).unwrap();

                        match request.packet_type {
                            rkon::PacketType::Command => {
                                if authenticated {
                                    let response = rkon::Packet {
                                        request_id: request.request_id,
                                        packet_type: rkon::PacketType::Command,
                                        body: "hello world!".to_string()
                                    };
                                    let encoded = response.encode();
                                    socket.write_all(&encoded).await.unwrap();
                                }
                            },
                            rkon::PacketType::Login => {
                                let config = get_config();
                                if request.body == config.world.rcon_password {
                                    authenticated = true;

                                    let response = rkon::Packet {
                                        request_id: request.request_id,
                                        packet_type: rkon::PacketType::Command,
                                        body: String::default()
                                    };
                                    let encoded = response.encode();
                                    socket.write_all(&encoded).await.unwrap();
                                } else {
                                    authenticated = false;

                                    let response = rkon::Packet {
                                        request_id: -1,
                                        packet_type: rkon::PacketType::Command,
                                        body: String::default()
                                    };
                                    let encoded = response.encode();
                                    socket.write_all(&encoded).await.unwrap();
                                }
                            },
                            _ => tracing::warn!("Ignoring unknown RCON packet")
                        }
                    }
                }
            }
        };
    }
}
