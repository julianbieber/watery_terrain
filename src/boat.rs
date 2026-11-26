//! Boats: a hull of two water-displacing spheres held apart by a joint, the
//! keel that keeps it moving along its length, and the boat entity that rides
//! the hull for a host to hang any model on.

use avian3d::prelude::{
    Collider, DistanceJoint, Forces, ReadRigidBodyForces, RigidBody, WriteRigidBodyForces,
};
use bevy::prelude::*;

use crate::{
    WateryTerrainSystems,
    chunks::Parked,
    water_sim::{WaterAtObjects, WaterDisplacement, submerged_fraction},
};

const KEEL_DRAG_PER_SECOND: f32 = 4.0;

/// A boat `length` metres from bow to stern, its hull two spheres of
/// `radius` metres, one at each end, that float, displace the water and are
/// carried by it like any [`WaterDisplacement`] object, held `length` apart.
///
/// Adding it spawns the hull around the entity's [`Transform`], bow towards
/// its forward (-z), and records it in [`BoatHull`]. From then on the
/// entity's transform follows the hull: centred between the spheres, forward
/// from stern to bow, up as close to world up as that allows. Its children
/// ride with it, so a model hung under it is drawn on the water; the entity
/// is hidden while its hull is parked outside the window.
///
/// A hull sphere moving across its boat through the water is slowed much
/// harder than one moving along it, so the boat goes where it points and
/// turns into a current. Despawning the boat despawns its hull.
#[derive(Component, Clone, Copy, Debug)]
#[require(Transform, Visibility)]
pub struct Boat {
    pub length: f32,
    pub radius: f32,
}

/// The rigid bodies of a [`Boat`]'s hull, for a host to push or steer it by
/// applying forces to them.
#[derive(Component, Clone, Copy, Debug)]
pub struct BoatHull {
    pub bow: Entity,
    pub stern: Entity,
}

pub(crate) struct BoatPlugin;

impl Plugin for BoatPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawn_hull);
        app.add_observer(despawn_hull);
        app.add_systems(Update, follow_hull.in_set(WateryTerrainSystems));
        app.add_systems(FixedUpdate, apply_keel.in_set(WateryTerrainSystems));
    }
}

fn spawn_hull(add: On<Add<Boat>>, mut commands: Commands, boats: Query<(&Boat, &Transform)>) {
    let Ok((boat, transform)) = boats.get(add.entity) else {
        return;
    };
    let half = transform.forward() * boat.length * 0.5;
    let mut sphere = |at: Vec3| {
        commands
            .spawn((
                Transform::from_translation(at),
                RigidBody::Dynamic,
                Collider::sphere(boat.radius),
                WaterDisplacement {
                    radius: boat.radius,
                },
            ))
            .id()
    };
    let bow = sphere(transform.translation + half);
    let stern = sphere(transform.translation - half);
    commands.entity(add.entity).insert((
        BoatHull { bow, stern },
        DistanceJoint::new(bow, stern).with_limits(boat.length, boat.length),
    ));
}

fn despawn_hull(remove: On<Remove<BoatHull>>, mut commands: Commands, hulls: Query<&BoatHull>) {
    if let Ok(hull) = hulls.get(remove.entity) {
        commands.entity(hull.bow).try_despawn();
        commands.entity(hull.stern).try_despawn();
    }
}

fn follow_hull(
    mut boats: Query<(&BoatHull, &mut Transform, &mut Visibility), With<Boat>>,
    hull: Query<(&Transform, Has<Parked>), Without<Boat>>,
) {
    for (boat_hull, mut transform, mut visibility) in &mut boats {
        let Ok([(bow, bow_parked), (stern, stern_parked)]) =
            hull.get_many([boat_hull.bow, boat_hull.stern])
        else {
            continue;
        };
        visibility.set_if_neq(if bow_parked || stern_parked {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
        *transform = hull_pose(bow.translation, stern.translation);
    }
}

fn hull_pose(bow: Vec3, stern: Vec3) -> Transform {
    Transform::from_translation((bow + stern) * 0.5).looking_to(bow - stern, Vec3::Y)
}

fn apply_keel(
    boats: Query<(&BoatHull, &Boat)>,
    mut spheres: Query<(Entity, Forces, &Transform), Without<Parked>>,
    water: Res<WaterAtObjects>,
) {
    for (hull, boat) in &boats {
        let Ok([bow, stern]) = spheres
            .get_many([hull.bow, hull.stern])
            .map(|[b, s]| [b.2.translation, s.2.translation])
        else {
            continue;
        };
        let along = (bow - stern).xz().normalize_or_zero();
        for sphere in [hull.bow, hull.stern] {
            let Ok((entity, mut forces, transform)) = spheres.get_mut(sphere) else {
                continue;
            };
            let Some(column) = water.0.get(&entity) else {
                continue;
            };
            let submerged = submerged_fraction(column, transform.translation.y, boat.radius);
            let slip = forces.linear_velocity().xz() - column.velocity;
            let push = keel_acceleration(slip, along, submerged);
            forces.apply_linear_acceleration(Vec3::new(push.x, 0.0, push.y));
        }
    }
}

fn keel_acceleration(slip: Vec2, along: Vec2, submerged: f32) -> Vec2 {
    let across = slip - along * slip.dot(along);
    -across * KEEL_DRAG_PER_SECOND * submerged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pose_sits_between_the_spheres_facing_the_bow() {
        let pose = hull_pose(Vec3::new(2.0, 1.0, 0.0), Vec3::new(-2.0, 1.0, 0.0));
        assert!(pose.translation.abs_diff_eq(Vec3::new(0.0, 1.0, 0.0), 1e-6));
        assert!(pose.forward().abs_diff_eq(Vec3::X, 1e-6));
        assert!(pose.up().abs_diff_eq(Vec3::Y, 1e-6));
    }

    #[test]
    fn pose_pitches_with_the_bow_raised() {
        let pose = hull_pose(Vec3::new(1.0, 1.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        assert!(pose.forward().y > 0.0);
        assert!(pose.up().y > 0.0);
    }

    #[test]
    fn keel_leaves_motion_along_the_boat_alone() {
        assert_eq!(
            keel_acceleration(Vec2::new(3.0, 0.0), Vec2::X, 1.0),
            Vec2::ZERO
        );
    }

    #[test]
    fn keel_resists_motion_across_the_boat() {
        let push = keel_acceleration(Vec2::new(1.0, 2.0), Vec2::X, 1.0);
        assert!(push.x == 0.0 && push.y < 0.0);
    }

    #[test]
    fn keel_out_of_the_water_does_nothing() {
        assert_eq!(
            keel_acceleration(Vec2::new(1.0, 2.0), Vec2::X, 0.0),
            Vec2::ZERO
        );
    }
}
