use bevy::prelude::*;

use crate::{render::FollowTerrainMarker, screens::Screen};

pub struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Gameplay), spawn_player_camera);
    }
}

fn spawn_player_camera(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(Screen::Gameplay),
        Camera3d::default(),
        Transform::from_translation(Vec3::new(0.0, 1000.0, -100.0)).looking_at(Vec3::ZERO, Vec3::Y),
        FollowTerrainMarker,
    ));
}
