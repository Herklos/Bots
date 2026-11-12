//! Read one regular file. A symlink is refused before `Workspace::read`, which follows it.
use super::ImageFault;
use crate::storage::workspace::{EntryKind, Workspace};
use std::io;

pub(crate) async fn read_regular(ws: &dyn Workspace, rel: &str) -> Result<Vec<u8>, ImageFault> {
    match ws.symlink_kind(rel).await {
        Ok(Some(EntryKind::File)) => {}
        Ok(Some(EntryKind::Symlink)) => return Err(ImageFault::Link),
        Ok(None | Some(EntryKind::Dir)) => return Err(ImageFault::Missing),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Err(ImageFault::Missing),
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => return Err(ImageFault::BadPath),
        Err(_) => return Err(ImageFault::Io),
    }
    match ws.read(rel).await {
        Ok(bytes) => Ok(bytes),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(ImageFault::Missing),
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => Err(ImageFault::BadPath),
        Err(_) => Err(ImageFault::Io),
    }
}
