use kodama::RECEIVE_BUFFER_SIZE;
use kodama::config::get_config;
use kodama::ipc::kodama::{CustomIpcData, CustomIpcSegment, CustomIpcType};
use kodama::ipc::lobby::ServiceAccount;
use kodama::ipc::lobby::{ClientLobbyIpcData, ServerLobbyIpcSegment};
use kodama::lobby::LobbyConnection;
use kodama::packet::{ConnectionType, send_custom_world_packet};
use kodama::packet::{PacketState, SegmentData, send_keep_alive};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = get_config();

    let addr = config.lobby.get_socketaddr();

    let listener = TcpListener::bind(addr).await.unwrap();

    tracing::info!("Server started on {addr}");

    let world_name = "Gilgamesh".to_string(); // TODO: hardcoded
    loop {
        let (socket, _) = listener.accept().await.unwrap();

        let state = PacketState { client_key: None };

        let mut connection = LobbyConnection {
            socket,
            state,
            session_id: None,
            stored_character_creation_name: String::new(),
            world_name: world_name.clone(),
            service_accounts: Vec::new(),
            selected_service_account: None,
        };

        tokio::spawn(async move {
            let mut buf = vec![0; RECEIVE_BUFFER_SIZE];
            loop {
                let n = connection
                    .socket
                    .read(&mut buf)
                    .await
                    .expect("Failed to read data!");

                if n != 0 {
                    let (segments, _) = connection.parse_packet(&buf[..n]);
                    for segment in &segments {
                        match &segment.data {
                            SegmentData::SecuritySetup { phrase, key } => {
                                connection.initialize_encryption(phrase, *key).await
                            }
                            SegmentData::KeepAliveRequest { id, timestamp } => {
                                send_keep_alive::<ServerLobbyIpcSegment>(
                                    &mut connection.socket,
                                    &mut connection.state,
                                    ConnectionType::Lobby,
                                    *id,
                                    *timestamp,
                                )
                                .await
                            }
                            SegmentData::KeepAliveResponse { .. } => {
                                // we can throw this away
                            }
                            SegmentData::Ipc { data } => match &data.data {
                                ClientLobbyIpcData::LoginEx {
                                    session_id,
                                    version_info,
                                    ..
                                } => {
                                    tracing::info!(
                                        "Client logging in! {session_id} {version_info}"
                                    );
                                    let config = get_config();

                                    let Ok(login_reply) = reqwest::get(format!(
                                        "http://{}/_private/service_accounts?sid={}",
                                        config.login.server_name, session_id
                                    ))
                                    .await
                                    else {
                                        tracing::warn!(
                                            "Failed to contact login server, is it running?"
                                        );
                                        break;
                                    };

                                    let Ok(body) = login_reply.text().await else {
                                        tracing::warn!(
                                            "Failed to contact login server, is it running?"
                                        );
                                        break;
                                    };

                                    let service_accounts: Option<Vec<ServiceAccount>> =
                                        serde_json::from_str(&body).ok();
                                    if let Some(service_accounts) = service_accounts {
                                        if service_accounts.is_empty() {
                                            tracing::warn!(
                                                "This account has no service accounts attached, how did this happen?"
                                            );
                                        } else {
                                            connection.service_accounts = service_accounts;
                                            connection.session_id = Some(session_id.clone());
                                            connection.send_account_list().await;
                                        }
                                    }

                                    connection.send_account_list().await;
                                }
                                ClientLobbyIpcData::ServiceLogin {
                                    sequence,
                                    account_index,
                                    ..
                                } => {
                                    connection.selected_service_account = Some(
                                        connection.service_accounts[*account_index as usize].id
                                            as u32,
                                    );
                                    connection.send_lobby_info(*sequence).await
                                }
                                ClientLobbyIpcData::CharaMake(chara_make) => {
                                    dbg!(chara_make);
                                    connection.handle_character_action(&chara_make).await;
                                }
                                ClientLobbyIpcData::GameLogin {
                                    sequence,
                                    content_id,
                                    ..
                                } => {
                                    tracing::info!("Client is joining the world with {content_id}");

                                    let our_actor_id;

                                    // find the actor id for this content id
                                    // NOTE: This is NOT the ideal solution. I theorize the lobby server has it's own records with this information.
                                    {
                                        let ipc_segment = CustomIpcSegment {
                                            unk1: 0,
                                            unk2: 0,
                                            op_code: CustomIpcType::GetActorId,
                                            option: 0,
                                            timestamp: 0,
                                            data: CustomIpcData::GetActorId {
                                                content_id: *content_id as u64,
                                            },
                                        };

                                        let response_segment =
                                            send_custom_world_packet(ipc_segment).await.unwrap();

                                        match &response_segment.data {
                                            CustomIpcData::ActorIdFound { actor_id } => {
                                                our_actor_id = *actor_id;
                                            }
                                            _ => panic!("Unexpected custom IPC packet type here!"),
                                        }
                                    }

                                    connection
                                        .send_enter_world(
                                            *sequence,
                                            *content_id as u64,
                                            our_actor_id,
                                        )
                                        .await;
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                }
            }
        });
    }
}
