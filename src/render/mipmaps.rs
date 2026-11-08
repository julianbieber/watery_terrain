//! Generates the mip chains of the surface textures when they load, for as
//! long as the textures are exported without them.

use bevy::{asset::AssetEvent, prelude::*, render::render_resource::TextureFormat};

const SURFACE_TEXTURE_ROOT: &str = "output/";

const MIN_LEVEL_SIDE: u32 = 16;

/// Adds mip levels to every RGBA8 image loaded from below `assets/output`.
pub struct SurfaceMipmapPlugin;

impl Plugin for SurfaceMipmapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, generate_on_load);
    }
}

fn generate_on_load(
    mut events: MessageReader<AssetEvent<Image>>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    for event in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = event else {
            continue;
        };
        let is_surface = asset_server
            .get_path(*id)
            .is_some_and(|path| path.path().starts_with(SURFACE_TEXTURE_ROOT));
        if !is_surface {
            continue;
        }
        if let Some(mut image) = images.get_mut(*id) {
            add_mips(&mut image);
        }
    }
}

fn add_mips(image: &mut Image) {
    let format = image.texture_descriptor.format;
    if !matches!(
        format,
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
    ) {
        warn!(?format, "surface texture left without mips");
        return;
    }
    let Some(base) = image.data.as_ref() else {
        return;
    };
    let size = image.texture_descriptor.size;
    let (data, levels) = with_mip_chain(base, size.width, size.height);
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
}

fn with_mip_chain(base: &[u8], width: u32, height: u32) -> (Vec<u8>, u32) {
    let mut data = base.to_vec();
    let mut level = base.to_vec();
    let (mut w, mut h) = (width, height);
    let mut levels = 1;
    while w.min(h) / 2 >= MIN_LEVEL_SIDE {
        level = halved(&level, w, h);
        w /= 2;
        h /= 2;
        data.extend_from_slice(&level);
        levels += 1;
    }
    (data, levels)
}

fn halved(level: &[u8], width: u32, height: u32) -> Vec<u8> {
    let (width, height) = (width as usize, height as usize);
    let (half_width, half_height) = (width / 2, height / 2);
    let mut out = Vec::with_capacity(half_width * half_height * 4);
    for y in 0..half_height {
        let rows = [y * 2, y * 2 + 1].map(|row| &level[row * width * 4..(row + 1) * width * 4]);
        for x in 0..half_width {
            for channel in 0..4 {
                let sum = [x * 8, x * 8 + 4]
                    .into_iter()
                    .flat_map(|texel| rows.map(|row| row[texel + channel] as u16))
                    .sum::<u16>();
                out.push(((sum + 2) / 4) as u8);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halving_averages_each_channel_over_two_by_two_texels() {
        let level = [
            0, 10, 20, 255, 4, 10, 20, 255, //
            8, 10, 20, 255, 12, 10, 20, 255,
        ];
        assert_eq!(halved(&level, 2, 2), vec![6, 10, 20, 255]);
    }

    #[test]
    fn chain_stops_at_the_smallest_side_and_appends_each_level() {
        let (data, levels) = with_mip_chain(&vec![7; 256 * 64 * 4], 256, 64);
        assert_eq!(levels, 3);
        assert_eq!(data.len(), (256 * 64 + 128 * 32 + 64 * 16) * 4);
        assert!(data.iter().all(|byte| *byte == 7));
    }

    #[test]
    fn texture_at_the_smallest_side_gets_no_levels() {
        assert_eq!(with_mip_chain(&[1; 16 * 16 * 4], 16, 16).1, 1);
    }
}
