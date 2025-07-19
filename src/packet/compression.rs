use binrw::{BinWrite, binrw};
use std::io::Cursor;

use binrw::{BinRead, BinResult};

use crate::packet::{PacketHeader, PacketSegment};

use super::{PacketState, ReadWriteIpcSegment};

#[binrw]
#[brw(repr = u8)]
#[derive(Debug, PartialEq)]
pub enum CompressionType {
    Uncompressed = 0,
    ZLib = 1,
    Oodle = 2,
}

#[binrw::parser(reader, endian)]
pub(crate) fn decompress<T: ReadWriteIpcSegment>(
    header: &PacketHeader,
    encryption_key: Option<&[u8]>,
) -> BinResult<Vec<PacketSegment<T>>> {
    let mut segments = Vec::new();

    let size = header.size as usize - std::mem::size_of::<PacketHeader>();

    let mut data = vec![0; size];
    reader.read_exact(&mut data).unwrap();

    let data = data; // TODO: implement compression

    let mut cursor = Cursor::new(&data);

    for _ in 0..header.segment_count {
        let current_position = cursor.position();
        segments.push(PacketSegment::read_options(
            &mut cursor,
            endian,
            (encryption_key,),
        )?);
        let new_position = cursor.position();
        let expected_size = segments.last().unwrap().calc_size() as u64;
        let actual_size = new_position - current_position;

        if expected_size != actual_size {
            tracing::warn!(
                "The segment {:#?} does not match the size in calc_size()! (expected {expected_size} got {actual_size}",
                segments.last()
            );
        }
    }

    Ok(segments)
}

pub(crate) fn compress<T: ReadWriteIpcSegment>(
    state: &mut PacketState,
    _compression_type: &CompressionType,
    segments: &[PacketSegment<T>],
) -> Vec<u8> {
    let mut segments_buffer = Vec::new();
    for segment in segments {
        let mut buffer = Vec::new();

        // write to buffer
        {
            let mut cursor = Cursor::new(&mut buffer);

            segment
                .write_le_args(
                    &mut cursor,
                    (state.client_key.as_ref().map(|s: &[u8; 16]| s.as_slice()),),
                )
                .unwrap();
        }

        segments_buffer.append(&mut buffer);
    }

    segments_buffer
}
