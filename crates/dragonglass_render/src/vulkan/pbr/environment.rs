use crate::vulkan::{
    core::{CommandPool, Context, ShaderCache},
    pbr::{Brdflut, HdrCubemap, IrradianceCubemap, PrefilterCubemap},
};
use anyhow::Result;
use log::info;

pub struct EnvironmentMapSet {
    pub brdflut: Brdflut,
    pub hdr: HdrCubemap,
    pub prefilter: PrefilterCubemap,
    pub irradiance: IrradianceCubemap,
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

        info!("Creating Prefilter cubemap");
        let prefilter = PrefilterCubemap::new(context, command_pool, shader_cache, &hdr.cubemap)?;

        info!("Creating Irradiance cubemap");
        let irradiance = IrradianceCubemap::new(context, command_pool, shader_cache, &hdr.cubemap)?;

        Ok(Self {
            brdflut,
            hdr,
            prefilter,
            irradiance,
        })
    }
}
