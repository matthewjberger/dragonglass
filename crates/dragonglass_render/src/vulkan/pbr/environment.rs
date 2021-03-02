use crate::vulkan::{
    core::{CommandPool, Context, ShaderCache},
    pbr::{Brdflut, HdrCubemap},
};
use anyhow::Result;
use log::info;

pub struct EnvironmentMapSet {
    pub brdflut: Brdflut,
    pub hdr: HdrCubemap,
}

impl EnvironmentMapSet {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        shader_cache: &mut ShaderCache,
    ) -> Result<Self> {
        info!("Creating Brdflut");
        let brdflut = Brdflut::new(context, command_pool, shader_cache)?;

        info!("Creating Hdr cubemap");
        let hdr = HdrCubemap::new(
            context,
            command_pool,
            "assets/skyboxes/desert.hdr",
            shader_cache,
        )?;

        Ok(Self { brdflut, hdr })
    }
}
