use crate::vulkan::{
    core::{CommandPool, Context, ShaderCache},
    pbr::Brdflut,
};
use anyhow::Result;
use log::info;

// FIXME_BRDFLUT: Add hdr map to this
// FIXME_BRDFLUT: Move out of world pipeline data and store in scene at high level
pub struct EnvironmentMapSet {
    pub brdflut: Brdflut,
}

impl EnvironmentMapSet {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        shader_cache: &mut ShaderCache,
    ) -> Result<Self> {
        info!("Creating Brdflut");
        let brdflut = Brdflut::new(context, command_pool, shader_cache)?;

        // FIXME_BRDFLUT: move hdr map in here too

        Ok(Self { brdflut })
    }
}
