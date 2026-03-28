use bevy::{
    asset::RenderAssetUsages,
    ecs::component::Component,
    image::Image,
    math::{Mat3, Vec2, Vec3, Vec3Swizzles},
    render::render_resource::{Extent3d, TextureUsages},
};

pub fn create_heightmap() -> Heightmap {
    let mut m = Heightmap::zero();

    for y in 0..Heightmap::DIM {
        for x in 0..Heightmap::DIM {
            let v = Vec2::new(x as f32 * 0.03, y as f32 * 0.03);
            let scope = mountain_noise(Vec3::new(v.y * 0.1, v.x * 0.1, 100.0));
            let h: f32 = mountain_noise(Vec3::new(v.x, v.y, 1.0)) * scope;
            m.set(x, y, h.abs());
        }
    }
    m
}

pub fn create_heightmap_spike() -> Heightmap {
    let mut m = Heightmap::zero();

    m.set(1024, 1024, 5.0);

    m
}

#[derive(Component)]
pub struct Heightmap {
    pub values: Vec<f32>,
}

impl Heightmap {
    pub const DIM: u32 = 128 * 16;
    fn zero() -> Heightmap {
        Heightmap {
            values: vec![0.0; (Self::DIM * Self::DIM) as usize],
        }
    }
    pub fn image(&self) -> Image {
        let data: Vec<u8> = self.values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut image = Image::new(
            Extent3d {
                width: Self::DIM,
                height: Self::DIM,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            data,
            bevy::render::render_resource::TextureFormat::R32Float,
            RenderAssetUsages::all(),
        );

        image.texture_descriptor.usage |= TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST;
        image
    }

    pub fn get(&self, x: u32, y: u32) -> f32 {
        assert!(x < Self::DIM);
        assert!(y < Self::DIM);

        let index = x as usize * (Self::DIM as usize) + y as usize;

        assert!(index < (Self::DIM * Self::DIM) as usize);

        self.values[index]
    }

    pub fn set(&mut self, x: u32, y: u32, h: f32) {
        assert!(x < Self::DIM);
        assert!(y < Self::DIM);

        let index = x as usize * (Self::DIM as usize) + y as usize;

        assert!(index < (Self::DIM * Self::DIM) as usize);

        self.values[index] = h;
    }
}

#[allow(dead_code)]
fn hash3(p: Vec2) -> Vec3 {
    let p3 = (Vec3::new(p.x, p.x, p.y) * 0.1031).fract();
    let p3 = p3 + p3.dot(Vec3::new(p3.y, p3.z, p3.x) + 33.33);
    ((p3.xxy() + p3.yzz()) * p3.zyx() * Vec3::splat(0.3183099)).fract()
}

#[allow(dead_code)]
fn voronoise(x: Vec2, u: f32, v: f32) -> f32 {
    let p = x.floor();
    let f = x.fract();

    let k = 1.0 + 63.0 * (1.0 - v).powf(4.0);
    let mut va = 0.0;
    let mut wt = 0.0;
    for j in -2..=2 {
        for i in -2..=2 {
            let g = Vec2::new(i as f32, j as f32);
            let o = hash3(p + g) * Vec3::new(u, u, 1.0);
            let r = g - f + o.xy();
            let d = r.dot(r);
            let w = (1.0 - smoothstep(0.0, 1.414, d.sqrt())).powf(k);
            va += w * o.z;
            wt += w;
        }
    }
    va / wt
}

#[allow(dead_code)]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = (x - edge0) / (edge1 - edge0);
    let clamped = t.max(0.0).min(1.0);
    clamped * clamped * (3.0 - 2.0 * clamped)
}

#[allow(dead_code)]
pub fn hash22(i: Vec2) -> Vec2 {
    let mut h = i * Vec2::new(127.1, 311.7);
    h = (h.fract() * 43758.5453123).fract();
    h
}

pub fn hash21(i: Vec2) -> f32 {
    let mut h = i.dot(Vec2::new(127.1, 311.7));
    h = (h.fract() * 43758.5453123).fract();
    h
}

#[allow(dead_code)]
pub fn gradient_noise(x: Vec2) -> f32 {
    let i = x.floor();
    let f = x.fract();
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    let ga = hash22(i + Vec2::ZERO);
    let gb = hash22(i + Vec2::new(1.0, 0.0));
    let gc = hash22(i + Vec2::new(0.0, 1.0));
    let gd = hash22(i + Vec2::new(1.0, 1.0));

    let va = ga.dot(f - Vec2::ZERO);
    let vb = gb.dot(f - Vec2::new(1.0, 0.0));
    let vc = gc.dot(f - Vec2::new(0.0, 1.0));
    let vd = gd.dot(f - Vec2::new(1.0, 1.0));

    va + u.x * (vb - va) + u.y * (vc - va) + u.x * u.y * (va - vb - vc + vd)
}

#[allow(dead_code)]
pub fn value_noise(x: Vec2) -> f32 {
    let p = x.floor();
    let w = x.fract();
    let u = w * w * w * (w * (w * 6.0 - 15.0) + 10.0);

    let a = hash21(p + Vec2::ZERO);
    let b = hash21(p + Vec2::X);
    let c = hash21(p + Vec2::new(0.0, 1.0));
    let d = hash21(p + Vec2::new(1.0, 1.0));

    let k0 = a;
    let k1 = b - a;
    let k2 = c - a;
    let k4 = a - b - c + d;

    -1.0 + 2.0 * (k0 + k1 * u.x + k2 * u.y + k4 * u.x * u.y)
}

#[allow(dead_code)]
fn rot(x: f32, y: f32, z: f32) -> Mat3 {
    Mat3::from_euler(bevy::math::EulerRot::XYZ, x, y, z)
}

#[allow(dead_code)]
fn gyroid(x: Vec3) -> f32 {
    let c = Vec3::new(x.x.cos(), x.y.cos(), x.z.cos());
    let s = Vec3::new(x.y.sin(), x.z.sin(), x.x.sin());
    c.dot(s)
}

#[allow(dead_code)]
fn dotnoise(mut x: Vec3) -> f32 {
    let mut a = 0.0;
    for _ in 0..4 {
        x = rot(0.1, 0.2, 0.3) * x;
        let v = gyroid(x);

        a += v * 0.25;
    }
    a
}

#[allow(dead_code)]
fn mountain_noise(x: Vec3) -> f32 {
    let mut a = 0.0;
    let mut f = 1.0;
    let mut amp = 1.5;
    for _ in 0..5 {
        a += dotnoise(x * f).abs() * amp;
        f *= 2.5;
        amp *= 0.5;
    }

    (1.0 - a).tanh()
}
