use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    pub graphics: Graphics,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Graphics {
    pub post_processing: PostProcessing,
}

#[derive(Default, Serialize, Deserialize)]
pub struct PostProcessing {
    pub film_grain: FilmGrain,
    pub chromatic_aberration: ChromaticAberration,
}

#[derive(Default, Serialize, Deserialize)]
pub struct ChromaticAberration {
    pub enabled: bool,
    pub strength: f32,
}

#[derive(Default, Serialize, Deserialize)]
pub struct FilmGrain {
    pub enabled: bool,
    pub strength: f32,
}
