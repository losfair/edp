use bitflags::bitflags;
use thiserror::Error;

#[derive(Error, Debug)]
#[error("protocol error")]
pub struct ProtocolError;

bitflags! {
    pub struct DistFlags: u64 {
      const DFLAG_PUBLISHED = 1;
      const DFLAG_ATOM_CACHE = 2;
      const DFLAG_EXTENDED_REFERENCES= 4;
      const DFLAG_DIST_MONITOR = 8;
      const DFLAG_FUN_TAGS = 0x10;
      const DFLAG_NEW_FUN_TAGS = 0x80;
      const DFLAG_EXTENDED_PIDS_PORTS = 0x100;
      const DFLAG_EXPORT_PTR_TAG = 0x200;
      const DFLAG_BIT_BINARIES = 0x400;
      const DFLAG_NEW_FLOATS = 0x800;
      const DFLAG_SMALL_ATOM_TAGS = 0x4000;
      const DFLAG_UTF8_ATOMS = 0x10000;
      const DFLAG_MAP_TAG = 0x20000;
      const DFLAG_BIG_CREATION = 0x40000;
      const DFLAG_HANDSHAKE_23 = 0x1000000;
      const DFLAG_UNLINK_ID = 0x2000000;
      const DFLAG_RESERVED = 0xfc000000;
      const DFLAG_NAME_ME = 0x2u64 << 32;
      const DFLAG_V4_NC = 0x4u64 << 32;
    }
}

impl Default for DistFlags {
  fn default() -> Self {
    Self::DFLAG_EXTENDED_REFERENCES
      | Self::DFLAG_EXTENDED_PIDS_PORTS
      | Self::DFLAG_FUN_TAGS
      | Self::DFLAG_NEW_FUN_TAGS
      | Self::DFLAG_EXPORT_PTR_TAG
      | Self::DFLAG_BIT_BINARIES
      | Self::DFLAG_NEW_FLOATS
      | Self::DFLAG_SMALL_ATOM_TAGS
      | Self::DFLAG_UTF8_ATOMS
      | Self::DFLAG_HANDSHAKE_23
      | Self::DFLAG_BIG_CREATION
      | Self::DFLAG_V4_NC
  }
}
