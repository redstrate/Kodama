use binrw::binrw;
use serde::{Deserialize, Serialize};

#[binrw]
#[derive(Debug, Serialize, Deserialize)]
pub struct CharaInfo {
    pub version: u32,
    pub unknown1: u32,
    pub tribe: u8,
    pub size: u8,
    pub hair_style: u16,
    pub hair_highlight_color: u8,
    pub hair_variation: u8,
    pub face_type: u8,
    pub characteristics: u8,
    pub characteristics_color: u8,

    pub unk1: u32,

    pub face_eyebrows: u8,
    pub face_iris_size: u8,
    pub face_eye_shape: u8,
    pub face_nose: u8,
    pub face_features: u8,
    pub face_mouth: u8,
    pub ears: u8,
    pub hair_color: u16,

    pub unk2: u32,

    pub skin_color: u16,
    pub eye_color: u16,

    pub voice: u8,
    pub guardian: u8,
    pub birth_month: u8,
    pub birth_day: u8,
    pub current_class: u8,

    pub unk3: u32,
    pub unk4: u32,
    pub unk5: u32,

    #[brw(pad_before = 0x10)]
    pub initial_town: u8,
}

#[cfg(not(target_family = "wasm"))]
impl rusqlite::types::FromSql for CharaInfo {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(serde_json::from_str(&String::column_result(value)?).unwrap())
    }
}
