use std::io::Cursor;

use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use binrw::{BinRead, BinWrite, binrw};
use bitflags::bitflags;
use modular_bitfield::prelude::*;

use crate::common::CHAR_NAME_MAX_LENGTH;

use super::{read_string, write_string};

#[binrw]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterFlag(u8);

bitflags! {
    impl CharacterFlag : u8 {
        const NONE = 0;
        /// "You cannot select this character with your current account."
        const LOCKED = 1;
        /// "A name change is required to log in with this character."
        const NAME_CHANGE_REQUIRED = 2;
        /// Not working?
        const MISSING_EXPANSION_FOR_LOGIN = 4;
        /// "To log in with this character you must first install A Realm Reborn". Depends on an expansion version of the race maybe?
        const MISSING_EXPANSION_FOR_EDIT = 8;
        /// Shows a DC traveling icon on the right, and changes the text on the left
        const DC_TRAVELING = 16;
        /// "This character is currently visiting the XYZ data center". ???
        const DC_TRAVELING_MESSAGE = 32;
    }
}

impl Default for CharacterFlag {
    fn default() -> Self {
        Self::NONE
    }
}

#[bitfield]
#[derive(Debug, Clone, Copy, Default, BinRead, BinWrite)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
pub struct FaceInfo {
    pub characteristics: B5,
    pub characteristics_color: B3,
    pub face_type: B6,
    pub ears: B2,
    pub mouth: B2,
    pub features: B2,
    pub nose: B3,
    pub eye_shape: B3,
    pub iris_size: B1,
    pub eyebrows: B3,
    pub unknown: B2,
}

#[binrw]
#[derive(Debug, Clone, Default)]
pub struct NeoClientSelectData {
    pub unk1: u32,
    pub unk2: u32,
    #[bw(calc = name.len() as u32 + 1)]
    pub name_length: u32,
    #[br(count = name_length)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub name: String,
    pub unk3: u32,
    pub unk4: u32,
    pub model: u32,
    pub height: u32,
    pub colors: u32,
    pub face: FaceInfo,
    pub hair: u32,
    pub voice: u32,
    pub main_hand: u32,
    #[brw(pad_after = 5)] // padding
    pub off_hand: u32,
    #[brw(pad_after = 8)] // padding
    pub model_ids: [u8; 13],
    pub unk5: u32,
    pub unk6: u32,
    pub current_class: u8,
    pub current_level: u16,
    pub current_job: u8,
    pub unk7: u16,
    pub tribe: u8,
    pub unk8: u32,
    #[bw(calc = location1.len() as u32 + 1)]
    pub location1_length: u32,
    #[br(count = location1_length)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub location1: String,
    #[bw(calc = location2.len() as u32 + 1)]
    pub location2_length: u32,
    #[br(count = location2_length)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub location2: String,
    pub guardian: u8,
    pub birth_month: u8,
    pub birth_day: u8,
    pub unk9: u16,
    pub unk10: u32,
    #[brw(pad_after = 16)] // padding
    pub unk11: u32,
    pub city_state: u32,
    pub city_state_again: u32,
}

impl NeoClientSelectData {
    pub fn to_string(&self) -> String {
        let mut cursor = Cursor::new(Vec::new());
        self.write_le(&mut cursor).unwrap();
        let bytes = cursor.into_inner();
        URL_SAFE.encode(&bytes)
    }
}

#[binrw]
#[derive(Debug, Clone, Default)]
pub struct CharacterDetails {
    pub unk2: u32,
    pub player_id: u32,
    pub index: u8,
    pub flags: CharacterFlag,
    pub unk1: u16,
    pub zone_id: u32,
    #[bw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
    #[br(count = CHAR_NAME_MAX_LENGTH)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub character_name: String,
    #[bw(pad_size_to = 14)]
    #[br(count = 14)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub server_name: String,
    #[bw(pad_size_to = 402)]
    #[br(count = 402)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub client_select_data: String,
}

impl CharacterDetails {
    pub const SIZE: usize = 464;
}

#[binrw]
#[derive(Debug, Clone, Default)]
pub struct ServiceLoginReply {
    pub sequence: u64,
    pub counter: u8,
    pub num_in_packet: u8,
    pub unk1: u8,
    #[brw(pad_after = 4)]
    pub unk3: u8,

    #[br(count = 2)]
    #[brw(pad_size_to = (CharacterDetails::SIZE * 2))]
    pub characters: Vec<CharacterDetails>,
}
