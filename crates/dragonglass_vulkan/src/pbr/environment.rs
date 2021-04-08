use std::path::Path;

use crate::{
    core::{CommandPool, Context, Cubemap, ShaderCache},
    pbr::{load_hdr_map, load_irradiance_map, load_prefilter_map, Brdflut},
};
use anyhow::Result;
use log::info;

pub struct EnvironmentMapSet {
    pub hdr: Cubemap,
    pub brdflut: Brdflut,
    pub prefilter: Cubemap,
    pub irradiance: Cubemap,
}

impl EnvironmentMapSet {
    pub fn new(
        context: &Context,
        command_pool: &CommandPool,
        shader_cache: &mut ShaderCache,
        path: impl AsRef<Path>,
    ) -> Result<Self> {
        info!("Creating Hdr cubemap");
        let hdr = load_hdr_map(context, command_pool, path, shader_cache)?;

        info!("Creating Brdflut");
        let brdflut = Brdflut::new(context, command_pool, shader_cache)?;

        info!("Creating Irradiance cubemap");
        let irradiance = load_irradiance_map(context, command_pool, shader_cache, &hdr)?;

        info!("Creating Prefilter cubemap");
        let prefilter = load_prefilter_map(context, command_pool, shader_cache, &hdr)?;

        Ok(Self {
            hdr,
            brdflut,
            prefilter,
            irradiance,
        })
    }
}
