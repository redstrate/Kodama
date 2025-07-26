use std::io::Cursor;

use crate::{
    common::CharaInfo,
    config::get_config,
    ipc::kodama::{CustomIpcData, CustomIpcSegment, CustomIpcType},
    packet::{
        CompressionType, ConnectionType, PacketSegment, SegmentData, SegmentType, send_packet,
    },
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use binrw::BinRead;

use super::ZoneConnection;

pub async fn handle_custom_ipc(connection: &mut ZoneConnection, data: &CustomIpcSegment) {
    match &data.data {
        CustomIpcData::RequestCreateCharacter {
            service_account_id,
            name,
            encoded,
            ..
        } => {
            tracing::info!("creating character from: {name} {encoded}");

            let mut cursor = Cursor::new(URL_SAFE.decode(encoded).unwrap());
            let chara_info = CharaInfo::read_le(&mut cursor).unwrap();

            dbg!(&chara_info);

            let (content_id, actor_id) = connection.database.create_player_data(
                *service_account_id,
                &name,
                &serde_json::to_string(&chara_info).unwrap(),
                chara_info.initial_town,
                0,
            );

            tracing::info!("Created new player: {content_id} {actor_id}");

            // send them the new actor and content id
            {
                connection
                    .send_segment(PacketSegment {
                        segment_type: SegmentType::KodamaIpc,
                        data: SegmentData::KodamaIpc {
                            data: CustomIpcSegment {
                                op_code: CustomIpcType::CharacterCreated,
                                data: CustomIpcData::CharacterCreated {
                                    actor_id,
                                    content_id,
                                },
                                ..Default::default()
                            },
                        },
                        ..Default::default()
                    })
                    .await;
            }
        }
        CustomIpcData::GetActorId { content_id } => {
            let actor_id = connection.database.find_actor_id(*content_id);

            tracing::info!("We found an actor id: {actor_id}");

            // send them the actor id
            {
                connection
                    .send_segment(PacketSegment {
                        segment_type: SegmentType::KodamaIpc,
                        data: SegmentData::KodamaIpc {
                            data: CustomIpcSegment {
                                op_code: CustomIpcType::ActorIdFound,
                                data: CustomIpcData::ActorIdFound { actor_id },
                                ..Default::default()
                            },
                        },
                        ..Default::default()
                    })
                    .await;
            }
        }
        CustomIpcData::CheckNameIsAvailable { name } => {
            let is_name_free = connection.database.check_is_name_free(name);

            // send response
            {
                connection
                    .send_segment(PacketSegment {
                        segment_type: SegmentType::KodamaIpc,
                        data: SegmentData::KodamaIpc {
                            data: CustomIpcSegment {
                                op_code: CustomIpcType::NameIsAvailableResponse,
                                data: CustomIpcData::NameIsAvailableResponse { free: is_name_free },
                                ..Default::default()
                            },
                        },
                        ..Default::default()
                    })
                    .await;
            }
        }
        CustomIpcData::RequestCharacterList { service_account_id } => {
            let config = get_config();

            let world_name = "Gilgamesh"; // TODO: hardcoded
            let characters = connection.database.get_character_list(
                *service_account_id,
                config.world.world_id,
                &world_name,
            );

            // send response
            {
                send_packet::<CustomIpcSegment>(
                    &mut connection.socket,
                    &mut connection.state,
                    ConnectionType::None,
                    CompressionType::Uncompressed,
                    &[PacketSegment {
                        segment_type: SegmentType::KodamaIpc,
                        data: SegmentData::KodamaIpc {
                            data: CustomIpcSegment {
                                op_code: CustomIpcType::RequestCharacterListRepsonse,
                                data: CustomIpcData::RequestCharacterListRepsonse { characters },
                                ..Default::default()
                            },
                        },
                        ..Default::default()
                    }],
                )
                .await;
            }
        }
        CustomIpcData::DeleteCharacter { content_id } => {
            connection.database.delete_character(*content_id);

            // send response
            {
                send_packet::<CustomIpcSegment>(
                    &mut connection.socket,
                    &mut connection.state,
                    ConnectionType::None,
                    CompressionType::Uncompressed,
                    &[PacketSegment {
                        segment_type: SegmentType::KodamaIpc,
                        data: SegmentData::KodamaIpc {
                            data: CustomIpcSegment {
                                op_code: CustomIpcType::CharacterDeleted,
                                data: CustomIpcData::CharacterDeleted { deleted: 1 },
                                ..Default::default()
                            },
                        },
                        ..Default::default()
                    }],
                )
                .await;
            }
        }
        _ => {
            panic!("The server is recieving a response or unknown custom IPC!")
        }
    }
}
