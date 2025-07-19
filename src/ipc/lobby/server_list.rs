use binrw::binrw;

use crate::common::{read_string, write_string};

#[binrw]
#[derive(Debug, Clone, Default)]
pub struct Server {
    pub id: u16,
    pub index: u16,
    pub population: u32,
    pub unk1: u64,
    #[bw(pad_size_to = 64)]
    #[br(count = 64)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub name: String,
}

impl Server {
    pub const SIZE: usize = 80;
}

#[binrw]
#[derive(Debug, Clone, Default)]
pub struct DistWorldInfo {
    pub sequence: u64,
    pub offset: u8,
    #[brw(pad_after = 3)] // padding
    pub num_servers: u32,
    #[br(count = 6)]
    #[brw(pad_size_to = 6 * Server::SIZE)]
    pub servers: Vec<Server>,
}
