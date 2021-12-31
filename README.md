# Dragonglass

Dragonglass is a 3D game engine written in Rust, using [wgpu](https://github.com/gfx-rs/wgpu) for rendering! 

> This project was developed using Vulkan and OpenGL for a while, but wgpu matured a lot in that time and is now a more sensible choice. Some of the screenshots may be from an older version of this engine that had a distinct Vulkan backend.

## Instructions

To run the visual editor for Dragonglass, run this command in the root directory:

```bash
# To choose the backend in Unix
# WGPU_BACKEND=vulkan
#
# To choose the backend in powershell
# $env:WGPU_BACKEND="vulkan"
cargo run --release --bin editor
```

## Gallery

![PBR](images/helmet.png)
![3D Picking](images/picking.gif)
![Selecting 3D objects](images/selections.gif)
