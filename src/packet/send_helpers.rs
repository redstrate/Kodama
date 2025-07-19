use std::io::Cursor;

use binrw::BinWrite;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::{
    RECEIVE_BUFFER_SIZE, common::timestamp_msecs, config::get_config, ipc::kodama::CustomIpcSegment,
};

use super::{
    CompressionType, ConnectionType, PacketHeader, PacketSegment, PacketState, ReadWriteIpcSegment,
    SegmentData, SegmentType, compression::compress, parse_packet,
};

pub async fn send_packet<T: ReadWriteIpcSegment>(
    socket: &mut TcpStream,
    state: &mut PacketState,
    connection_type: ConnectionType,
    compression_type: CompressionType,
    segments: &[PacketSegment<T>],
) {
    let data = compress(state, &compression_type, segments);
    let size = std::mem::size_of::<PacketHeader>() + data.len();

    let header = PacketHeader {
        timestamp: timestamp_msecs(),
        size: size as u16,
        connection_type,
        segment_count: segments.len() as u16,
        is_authenticated: 0,
        compressed_or_encoded: 0,
    };

    dbg!(&header);

    let mut cursor = Cursor::new(Vec::new());
    header.write_le(&mut cursor).unwrap();
    std::io::Write::write_all(&mut cursor, &data).unwrap();

    let buffer = cursor.into_inner();

    std::fs::write("/home/josh/out.bin", &buffer).unwrap();

    if let Err(e) = socket.write_all(&buffer).await {
        tracing::warn!("Failed to send packet: {e}");
    }
}

pub async fn send_keep_alive<T: ReadWriteIpcSegment>(
    socket: &mut TcpStream,
    state: &mut PacketState,
    connection_type: ConnectionType,
    id: u32,
    timestamp: u32,
) {
    let response_packet: PacketSegment<T> = PacketSegment {
        segment_type: SegmentType::KeepAliveResponse,
        data: SegmentData::KeepAliveResponse { id, timestamp },
        ..Default::default()
    };
    send_packet(
        socket,
        state,
        connection_type,
        CompressionType::Uncompressed,
        &[response_packet],
    )
    .await;
}

/// Sends a custom IPC packet to the world server, meant for private server-to-server communication.
/// Returns the first custom IPC segment returned.
pub async fn send_custom_world_packet(segment: CustomIpcSegment) -> Option<CustomIpcSegment> {
    let config = get_config();

    let addr = config.world.get_public_socketaddr();

    let mut stream = TcpStream::connect(addr).await.unwrap();

    let mut packet_state = PacketState { client_key: None };

    let segment: PacketSegment<CustomIpcSegment> = PacketSegment {
        segment_type: SegmentType::KodamaIpc,
        data: SegmentData::KodamaIpc { data: segment },
        ..Default::default()
    };

    send_packet(
        &mut stream,
        &mut packet_state,
        ConnectionType::None,
        CompressionType::Uncompressed,
        &[segment],
    )
    .await;

    // read response
    let mut buf = vec![0; RECEIVE_BUFFER_SIZE];
    let n = stream.read(&mut buf).await.expect("Failed to read data!");
    if n != 0 {
        let (segments, _) = parse_packet::<CustomIpcSegment>(&buf[..n], &mut packet_state);

        return match &segments[0].data {
            SegmentData::KodamaIpc { data } => Some(data.clone()),
            _ => None,
        };
    }

    None
}
