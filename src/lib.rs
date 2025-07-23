//! A server replacement for a certain MMO.

#![allow(clippy::large_enum_variant)]

use patch::Version;

/// The blowfish implementation used for packet encryption.
pub mod blowfish;

/// Common functions, structures used between all servers.
pub mod common;

/// Config management.
pub mod config;

/// Lobby server-specific code.
#[cfg(not(target_family = "wasm"))]
pub mod lobby;

/// World server-specific code.
#[cfg(not(target_family = "wasm"))]
pub mod world;

/// Everything packet parsing related.
pub mod packet;

/// Logic server-specific code.
#[cfg(not(target_family = "wasm"))]
pub mod login;

/// Patch server-specific code.
pub mod patch;

/// Opcodes, see `resources/opcodes.json`
pub mod opcodes;

/// IPC
pub mod ipc;

/// Used in the encryption key.
const GAME_VERSION: u16 = 1000;

pub const RECEIVE_BUFFER_SIZE: usize = 32000;

/// Supported boot version.
pub const SUPPORTED_BOOT_VERSION: Version = Version("2010.09.18.0000");

/// Supported game version.
pub const SUPPORTED_GAME_VERSION: Version = Version("2012.09.19.0001");
