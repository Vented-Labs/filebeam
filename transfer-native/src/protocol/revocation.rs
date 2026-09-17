use anyhow::Result;

use crate::control::Control;

use super::upload;

pub(super) fn run(id: &str, control: &Control) -> Result<()> {
    let store = control.open_checkpoint_store(id)?;
    upload::revoke(store, control)
}
