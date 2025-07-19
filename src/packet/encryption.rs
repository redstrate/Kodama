use std::io::Cursor;

use binrw::BinResult;

use crate::{GAME_VERSION, blowfish::Blowfish};

use super::{IPC_HEADER_SIZE, ReadWriteIpcSegment};

pub fn generate_encryption_key(key: u32, phrase: &str) -> [u8; 16] {
    let mut base_key = vec![0x78, 0x56, 0x34, 0x12];
    base_key.extend_from_slice(&key.to_le_bytes());
    base_key.extend_from_slice(&GAME_VERSION.to_le_bytes());
    base_key.extend_from_slice(&[0; 2]); // padding (possibly for game version?)
    base_key.extend_from_slice(phrase.as_bytes());

    base_key.resize(0x2C, 0x0);

    md5::compute(&base_key).0
}

#[binrw::parser(reader, endian)]
pub(crate) fn decrypt<T: ReadWriteIpcSegment>(
    size: u32,
    encryption_key: Option<&[u8]>,
) -> BinResult<T> {
    dbg!(encryption_key);
    if let Some(encryption_key) = encryption_key {
        let size = size - IPC_HEADER_SIZE;

        let mut data = vec![0; size as usize];
        reader.read_exact(&mut data)?;

        let blowfish = Blowfish::new(encryption_key);
        blowfish.decrypt(&mut data);

        let mut cursor = Cursor::new(&data);
        T::read_options(&mut cursor, endian, (&size,))
    } else {
        T::read_options(reader, endian, (&size,))
    }
}

#[binrw::writer(writer, endian)]
pub(crate) fn encrypt<T: ReadWriteIpcSegment>(
    value: &T,
    size: u32,
    encryption_key: Option<&[u8]>,
) -> BinResult<()> {
    if let Some(encryption_key) = encryption_key {
        let size = size - IPC_HEADER_SIZE;

        let mut cursor = Cursor::new(Vec::new());
        value.write_options(&mut cursor, endian, ())?;

        let mut buffer = cursor.into_inner();
        buffer.resize(size as usize, 0);

        let blowfish = Blowfish::new(encryption_key);
        blowfish.encrypt(&mut buffer);

        writer.write_all(&buffer)?;

        Ok(())
    } else {
        value.write_options(writer, endian, ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_key() {
        let key = generate_encryption_key(1752899183, "Test Ticket Data");
        assert_eq!(
            key,
            [
                220, 71, 191, 113, 220, 81, 26, 160, 233, 114, 234, 231, 242, 221, 168, 86
            ]
        );
    }
}
