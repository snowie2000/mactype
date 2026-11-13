//! The bookkeeping over the generation directories, their receipts and the
//! migration pins: staging and verifying a payload, keeping the generations
//! retention must preserve, and removing only an installation every part of
//! which carries this installer's receipt. The methods live next to the code
//! they own (deployment, retention, uninstall); this type is the one owner
//! they hang off, so no generation is written or removed except through it.

use mactype_service_contract::MachinePaths;

pub(super) struct RuntimeGenerationStore<'a> {
    pub(super) paths: &'a MachinePaths,
}

impl<'a> RuntimeGenerationStore<'a> {
    pub(super) const fn new(paths: &'a MachinePaths) -> Self {
        Self { paths }
    }
}
