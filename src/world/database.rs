use std::sync::Mutex;

use rusqlite::Connection;

use crate::{
    common::{CharaInfo, Position},
    ipc::lobby::{CharacterDetails, CharacterFlag, FaceInfo, NeoClientSelectData},
};

pub struct WorldDatabase {
    connection: Mutex<Connection>,
}

pub struct CharacterData {
    pub name: String,
    pub city_state: u8,
    pub position: Position,
    pub zone_id: u16,
}

impl Default for WorldDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldDatabase {
    pub fn new() -> Self {
        let connection = Connection::open("world.db").expect("Failed to open database!");

        // Create characters table
        {
            let query = "CREATE TABLE IF NOT EXISTS characters (content_id INTEGER PRIMARY KEY, service_account_id INTEGER, actor_id INTEGER);";
            connection.execute(query, ()).unwrap();
        }

        // Create characters data table
        {
            let query = "CREATE TABLE IF NOT EXISTS character_data
                (content_id INTEGER PRIMARY KEY,
                name STRING,
                chara_info STRING,
                city_state INTEGER,
                zone_id INTEGER,
                classjob_id INTEGER,
                pos_x REAL,
                pos_y REAL,
                pos_z REAL,
                rotation REAL);";
            connection.execute(query, ()).unwrap();
        }

        Self {
            connection: Mutex::new(connection),
        }
    }

    pub fn find_actor_id(&self, content_id: u64) -> u32 {
        let connection = self.connection.lock().unwrap();

        let mut stmt = connection
            .prepare("SELECT actor_id FROM characters WHERE content_id = ?1")
            .unwrap();

        stmt.query_row((content_id,), |row| row.get(0)).unwrap()
    }

    pub fn get_character_list(
        &self,
        service_account_id: u32,
        world_id: u16,
        world_name: &str,
    ) -> Vec<CharacterDetails> {
        let connection = self.connection.lock().unwrap();

        let content_actor_ids: Vec<(u32, u32)>;

        // find the content ids associated with the service account
        {
            let mut stmt = connection
                .prepare(
                    "SELECT content_id, actor_id FROM characters WHERE service_account_id = ?1",
                )
                .unwrap();

            content_actor_ids = stmt
                .query_map((service_account_id,), |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .map(|x| x.unwrap())
                .collect();
        }

        let mut characters = Vec::new();

        for (index, (content_id, actor_id)) in content_actor_ids.iter().enumerate() {
            let mut stmt = connection
                .prepare(
                    "SELECT name, chara_info, zone_id, classjob_id FROM character_data WHERE content_id = ?1",
                )
                .unwrap();

            struct CharaListQuery {
                name: String,
                chara_info: CharaInfo,
                zone_id: u16,
                classjob_id: i32,
            }

            let result: Result<CharaListQuery, rusqlite::Error> =
                stmt.query_row((content_id,), |row| {
                    Ok(CharaListQuery {
                        name: row.get(0)?,
                        chara_info: row.get(1)?,
                        zone_id: row.get(2)?,
                        classjob_id: row.get(3)?,
                    })
                });

            if let Ok(query) = result {
                characters.push(CharacterDetails {
                    unk2: 0,
                    player_id: *actor_id, // TODO: not correct
                    index: index as u8,
                    flags: CharacterFlag::NONE,
                    zone_id: 0,
                    unk1: 0,
                    character_name: query.name.clone(),
                    server_name: world_name.to_string(),
                    client_select_data: NeoClientSelectData {
                        unk1: 0x000004c0,
                        unk2: 0x232327ea,
                        name: query.name.clone(),
                        unk3: 0x1c,
                        unk4: 0x04,
                        model: 1,
                        height: query.chara_info.size as u32,
                        colors: query.chara_info.skin_color as u32
                            | (query.chara_info.hair_style << 10) as u32
                            | ((query.chara_info.eye_color as u32) << 20),
                        face: FaceInfo::new()
                            .with_characteristics(query.chara_info.characteristics)
                            .with_characteristics_color(query.chara_info.characteristics_color)
                            .with_face_type(query.chara_info.face_type)
                            .with_ears(query.chara_info.ears)
                            .with_features(query.chara_info.face_features)
                            .with_eyebrows(query.chara_info.face_eyebrows)
                            .with_eye_shape(query.chara_info.face_eye_shape)
                            .with_iris_size(query.chara_info.face_iris_size)
                            .with_mouth(query.chara_info.face_mouth)
                            .with_nose(query.chara_info.face_nose),
                        hair: query.chara_info.hair_highlight_color as u32
                            | (query.chara_info.hair_variation << 5) as u32
                            | (query.chara_info.hair_style << 10) as u32,
                        voice: query.chara_info.voice as u32,
                        main_hand: 0,
                        off_hand: 0,
                        model_ids: [0; 13],
                        unk5: 1,
                        unk6: 1,
                        current_class: 0,
                        current_level: 0,
                        current_job: 0,
                        unk7: 1,
                        tribe: query.chara_info.tribe,
                        unk8: 0xe22222aa,
                        location1: "prv0Inn01".to_string(),
                        location2: "defaultTerritory".to_lowercase(),
                        guardian: query.chara_info.guardian,
                        birth_month: query.chara_info.birth_month,
                        birth_day: query.chara_info.birth_day,
                        unk9: 0x17,
                        unk10: 4,
                        unk11: 4,
                        city_state: query.chara_info.initial_town as u32,
                        city_state_again: query.chara_info.initial_town as u32,
                    }
                    .to_string(),
                });
            }
        }

        characters
    }

    fn generate_content_id() -> u32 {
        fastrand::u32(..)
    }

    fn generate_actor_id() -> u32 {
        fastrand::u32(..)
    }

    /// Gives (content_id, actor_id)
    pub fn create_player_data(
        &self,
        service_account_id: u32,
        name: &str,
        chara_make_str: &str,
        city_state: u8,
        zone_id: u16,
    ) -> (u64, u32) {
        let content_id = Self::generate_content_id();
        let actor_id = Self::generate_actor_id();

        let connection = self.connection.lock().unwrap();

        // insert ids
        connection
            .execute(
                "INSERT INTO characters VALUES (?1, ?2, ?3);",
                (content_id, service_account_id, actor_id),
            )
            .unwrap();

        // insert char data
        connection
            .execute(
                "INSERT INTO character_data VALUES (?1, ?2, ?3, ?4, ?5, 0, 0.0, 0.0, 0.0, 0.0);",
                (content_id, name, chara_make_str, city_state, zone_id),
            )
            .unwrap();

        (content_id as u64, actor_id)
    }

    /// Checks if `name` is in the character data table
    pub fn check_is_name_free(&self, name: &str) -> bool {
        let connection = self.connection.lock().unwrap();

        let mut stmt = connection
            .prepare("SELECT content_id FROM character_data WHERE name = ?1")
            .unwrap();

        !stmt.exists((name,)).unwrap()
    }

    /// Deletes a character and all associated data
    pub fn delete_character(&self, content_id: u64) {
        let connection = self.connection.lock().unwrap();

        // delete data
        {
            let mut stmt = connection
                .prepare("DELETE FROM character_data WHERE content_id = ?1")
                .unwrap();
            stmt.execute((content_id,)).unwrap();
        }

        // delete char
        {
            let mut stmt = connection
                .prepare("DELETE FROM characters WHERE content_id = ?1")
                .unwrap();
            stmt.execute((content_id,)).unwrap();
        }
    }
}
