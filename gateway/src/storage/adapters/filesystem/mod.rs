//! The file system adapter: Bots live in a folder on this machine.
pub(crate) mod atomic;
mod bot_overlay;
mod bots;
mod connectors;
pub(crate) mod copy;
pub(crate) mod folder;
mod local_disk;
mod local_state;
mod records;
mod rooms;
mod rooms_overlay;
mod rooms_restore;
mod schedules;
mod settings;

pub(crate) use atomic::{read_json, write_atomic, write_json, write_json_private};
pub(crate) use bots::FilesystemBotStore;
pub(crate) use connectors::FilesystemConnectorStore;
pub(crate) use local_disk::LocalDisk;
pub(crate) use local_state::FilesystemLocalStateStore;
pub(crate) use records::FilesystemRecordStore;
pub(crate) use rooms::FilesystemRoomStore;
pub(crate) use schedules::FilesystemScheduleStore;
pub(crate) use settings::FilesystemSettingsStore;
