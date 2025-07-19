use std::io::Cursor;

use binrw::{BinRead, binrw};

use crate::{
    common::{read_string, write_string},
    ipc::kodama::CustomIpcSegment,
    packet::encryption::decrypt,
};

use super::{compression::decompress, encryption::encrypt, ipc::ReadWriteIpcSegment};

#[binrw]
#[brw(repr = u16)]
#[derive(Debug, PartialEq)]
pub enum ConnectionType {
    None = 0x0,
    Zone = 0x1,
    Chat = 0x2,
    Lobby = 0x3,
}

#[binrw]
#[brw(repr = u16)]
#[derive(Debug, PartialEq, Copy, Clone, Default)]
pub enum SegmentType {
    #[default]
    None = 0x0,
    Setup = 0x1,
    Initialize = 0x2,
    // Also known as "UPLAYER"
    Ipc = 0x3,
    KeepAliveRequest = 0x7,
    KeepAliveResponse = 0x8,
    // Also known as "SECSETUP"
    SecuritySetup = 0x9,
    // Also known as "SECINIT"
    SecurityInitialize = 0xA,
    // This isn't in retail!
    KodamaIpc = 0xAAAA,
}

#[binrw]
#[brw(import(kind: SegmentType, size: u32, encryption_key: Option<&[u8]>))]
#[derive(Debug, Clone)]
pub enum SegmentData<T: ReadWriteIpcSegment> {
    #[br(pre_assert(kind == SegmentType::None))]
    None(),
    #[br(pre_assert(kind == SegmentType::Setup))]
    Setup {
        #[brw(pad_before = 4)] // empty
        #[brw(pad_size_to = 36)]
        #[br(count = 36)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        actor_id: String, // square enix in their infinite wisdom has this as a STRING REPRESENTATION of an integer. what
    },
    #[br(pre_assert(kind == SegmentType::Initialize))]
    Initialize {
        actor_id: u32,
        #[brw(pad_after = 32)]
        timestamp: u32,
    },
    #[br(pre_assert(kind == SegmentType::SecuritySetup))]
    SecuritySetup {
        #[brw(pad_before = 36)] // empty
        #[brw(pad_size_to = 64)]
        #[br(count = 64)]
        #[br(map = read_string)]
        #[bw(ignore)]
        phrase: String,

        #[brw(pad_after = 512)] // empty
        key: u32,
    },
    #[br(pre_assert(kind == SegmentType::Ipc))]
    Ipc {
        #[br(parse_with = decrypt, args(size, encryption_key))]
        #[bw(write_with = encrypt, args(size, encryption_key))]
        data: T,
    },
    #[br(pre_assert(kind == SegmentType::KeepAliveRequest))]
    KeepAliveRequest { id: u32, timestamp: u32 },
    #[br(pre_assert(kind == SegmentType::SecurityInitialize))]
    SecurityInitialize {
        #[br(count = 640)]
        #[brw(pad_size_to = 640)]
        data: Vec<u8>,
    },
    #[br(pre_assert(kind == SegmentType::KeepAliveResponse))]
    KeepAliveResponse { id: u32, timestamp: u32 },

    #[br(pre_assert(kind == SegmentType::KodamaIpc))]
    KodamaIpc {
        #[br(args(&0))] // this being zero is okay, custom ipc segments don't use the size arg
        data: CustomIpcSegment,
    },
}

impl<T: ReadWriteIpcSegment> Default for SegmentData<T> {
    fn default() -> Self {
        Self::None()
    }
}

#[binrw]
#[derive(Debug)]
pub struct PacketHeader {
    pub is_authenticated: u8,
    pub compressed_or_encoded: u8,
    pub connection_type: ConnectionType,
    pub size: u16,
    pub segment_count: u16,
    pub timestamp: u64,
}

#[binrw]
#[brw(import(encryption_key: Option<&[u8]>))]
#[derive(Debug, Clone)]
pub struct PacketSegment<T: ReadWriteIpcSegment> {
    #[bw(calc = self.calc_size() as u16)] // TODO: switch to u16 everywhere
    pub size: u16,
    pub segment_type: SegmentType,
    pub source_actor: u32,
    #[brw(pad_after = 4)] // unknown, but not empty i guess
    pub target_actor: u32,
    #[bw(args(*segment_type, size as u32, encryption_key))]
    #[br(args(segment_type, size as u32, encryption_key))]
    #[br(err_context("segment size = {}", size))]
    pub data: SegmentData<T>,
}

impl<T: ReadWriteIpcSegment> Default for PacketSegment<T> {
    fn default() -> Self {
        Self {
            source_actor: 0,
            target_actor: 0,
            segment_type: SegmentType::default(),
            data: SegmentData::default(),
        }
    }
}

impl<T: ReadWriteIpcSegment> PacketSegment<T> {
    pub fn calc_size(&self) -> u32 {
        let header = std::mem::size_of::<u32>() * 4;
        header as u32
            + match &self.data {
                SegmentData::None() => 0,
                SegmentData::SecuritySetup { .. } => 616,
                SegmentData::SecurityInitialize { .. } => 640,
                SegmentData::Ipc { data } => data.calc_size(),
                SegmentData::KeepAliveRequest { .. } => 0x8,
                SegmentData::KeepAliveResponse { .. } => 0x8,
                SegmentData::Initialize { .. } => 40,
                SegmentData::Setup { .. } => 40,
                SegmentData::KodamaIpc { data } => data.calc_size(),
            }
    }
}

#[binrw]
#[brw(import(encryption_key: Option<&[u8]>))]
#[derive(Debug)]
struct Packet<T: ReadWriteIpcSegment> {
    header: PacketHeader,
    #[bw(args(encryption_key))]
    #[br(parse_with = decompress, args(&header, encryption_key,))]
    segments: Vec<PacketSegment<T>>,
}

// temporary
/// State needed for each connection between the client & server, containing various things like the compressor and encryption keys.
pub struct PacketState {
    pub client_key: Option<[u8; 16]>,
}

pub fn parse_packet<T: ReadWriteIpcSegment>(
    data: &[u8],
    state: &mut PacketState,
) -> (Vec<PacketSegment<T>>, ConnectionType) {
    let mut cursor = Cursor::new(data);

    match Packet::read_le_args(
        &mut cursor,
        (state.client_key.as_ref().map(|s: &[u8; 16]| s.as_slice()),),
    ) {
        Ok(packet) => (packet.segments, packet.header.connection_type),
        Err(err) => {
            tracing::error!("{err}");

            (Vec::new(), ConnectionType::None)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::packet::IpcSegment;
    use binrw::BinWrite;

    use super::*;

    #[test]
    fn test_packet_header() {
        let mut cursor = Cursor::new(vec![
            0x00, 0x00, 0x00, 0x00, 0xA0, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ]);
        let header = PacketHeader::read_le(&mut cursor).unwrap();

        assert_eq!(header.is_authenticated, 0);
        assert_eq!(header.compressed_or_encoded, 0);
        assert_eq!(header.connection_type, ConnectionType::None);
        assert_eq!(header.size, 672);
        assert_eq!(header.segment_count, 1);
        assert_eq!(header.timestamp, 0);
    }

    /// Ensure that the packet size as reported matches up with what we write
    #[test]
    fn test_packet_sizes() {
        #[binrw]
        #[brw(repr = u16)]
        #[derive(Clone, PartialEq, Debug)]
        enum ClientLobbyIpcType {
            Dummy = 0x1,
        }

        #[binrw]
        #[br(import(_magic: &ClientLobbyIpcType, _size: &u32))]
        #[derive(Debug, Clone)]
        enum ClientLobbyIpcData {
            Dummy(),
        }

        type ClientLobbyIpcSegment = IpcSegment<ClientLobbyIpcType, ClientLobbyIpcData>;

        impl ReadWriteIpcSegment for ClientLobbyIpcSegment {
            fn calc_size(&self) -> u32 {
                todo!()
            }

            fn get_name(&self) -> &'static str {
                todo!()
            }

            fn get_opcode(&self) -> u16 {
                todo!()
            }
        }

        let packet_types = [
            SegmentData::SecuritySetup {
                phrase: String::new(),
                key: 0,
            },
            SegmentData::SecurityInitialize { data: Vec::new() },
            SegmentData::KeepAliveRequest {
                id: 0,
                timestamp: 0,
            },
            SegmentData::KeepAliveResponse {
                id: 0,
                timestamp: 0,
            },
        ];

        for packet in &packet_types {
            let mut cursor = Cursor::new(Vec::new());

            let packet_segment: PacketSegment<ClientLobbyIpcSegment> = PacketSegment {
                segment_type: SegmentType::None,
                data: packet.clone(),
                ..Default::default()
            };
            packet_segment.write_le(&mut cursor).unwrap();

            let buffer = cursor.into_inner();

            assert_eq!(buffer.len(), packet_segment.calc_size() as usize);
        }
    }
}
