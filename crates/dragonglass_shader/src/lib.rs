use glob::glob;
use log::{error, info};
use std::{error::Error, io, path::Path, process::Command};

type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

const SHADER_COMPILER_NAME: &str = "glslangValidator";

pub fn compile_shaders(shader_glob: &str) -> Result<()> {
    for shader_path in glob(shader_glob)?.flatten() {
        compile_shader(&shader_path)?;
    }
    Ok(())
}

fn compile_shader(shader_path: &Path) -> Result<()> {
    let parent_name = shader_path
        .parent()
        .ok_or("Failed to get shader parent directory name")?;

    let file_name = shader_path.file_name().ok_or("Failed to get file_name")?;

    let output_name = file_name
        .to_str()
        .ok_or("Failed to convert file_name os_str to string")?
        .replace("glsl", "spv");

    info!("Compiling {:?} -> {:?}", file_name, output_name);
    let result = Command::new(SHADER_COMPILER_NAME)
        .current_dir(parent_name)
        .arg("-V")
        .arg(file_name)
        .arg("-o")
        .arg(output_name)
        .output();

    log_compilation_result(result)?;

    Ok(())
}

fn log_compilation_result(result: io::Result<std::process::Output>) -> Result<()> {
    match result {
        Ok(output) if !output.status.success() => {
            error!(
                "Shader compilation output: {}",
                String::from_utf8(output.stdout)?
            );
            error!("Failed to compile shader: {}", output.status);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => error!(
            "Failed to find the shader compiler program: '{}'",
            SHADER_COMPILER_NAME
        ),
        Err(error) => error!("Failed to compile shader: {}", error),
        _ => {}
    };
    Ok(())
}
