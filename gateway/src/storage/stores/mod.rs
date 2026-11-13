//! Domain storage interfaces. Each adapter owns persistence, not validation.
mod bots;
mod connectors;
mod local_state;
mod records;
mod rooms;
mod schedules;
mod settings;
pub(crate) use bots::{valid_slug, BotStore};
pub(crate) use connectors::{ConnectorStore, EmptyDocument};
pub(crate) use local_state::LocalStateStore;
pub(crate) use records::RecordStore;
pub(crate) use rooms::RoomStore;
pub(crate) use schedules::ScheduleStore;
pub(crate) use settings::SettingsStore;
