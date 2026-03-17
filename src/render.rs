use bevy::prelude::*;

pub struct TerrainRanderPlugin;

impl Plugin for TerrainRanderPlugin {
    fn build(&self, app: &mut App) {
        todo!()
    }
}

#[derive(Component)]
pub struct TerrainHeightMap {
    pub half_length: f32,
    pub division_levels: u8,
    pub steps_per_level: u8,
}

impl TerrainHeightMap {
    fn create_base_mesh(&self) -> Mesh {
        let top_left = Vec3::new(-self.half_length, self.half_length, 0.0);
        let bottom_left = Vec3::new(-self.half_length, -self.half_length, 0.0);

        let top_right = Vec3::new(self.half_length, self.half_length, 0.0);
        let bottom_right = Vec3::new(self.half_length, -self.half_length, 0.0);

        todo!()
    }

    fn division_level(&self, p: Vec3) -> u8 {
        let d = TerrainHeightMap::center_distance(p);
        let d = (d / self.half_length) as u8;
        d
    }

    fn center_distance(a: Vec3) -> f32 {
        let a = a.abs();
        a.x.max(a.y)
    }

    fn step_right(division_level: u8) -> Vec3 {
        todo!()
    }
}
