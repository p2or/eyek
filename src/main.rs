use bvh::aabb::{Bounded, AABB};
use bvh::bounding_hierarchy::BHShape;
use bvh::bvh::BVH;
use bvh::nalgebra::distance;
use bvh::nalgebra::geometry::{Isometry3, Perspective3, Translation3, UnitQuaternion};
use bvh::nalgebra::{Point3, Vector3};
use bvh::ray::Ray;
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use obj;
use rayon::prelude::*;
use serde_derive::Deserialize;
use serde_json;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
struct Tris2D {
    a: Point3<f32>,
    b: Point3<f32>,
    c: Point3<f32>,
}
impl Tris2D {
    /*
    fn has_point(&self, pt: Point3<f32>) -> bool {
        fn sign(a: Point3<f32>, b: Point3<f32>, c: Point3<f32>) -> f32 {
            (a.x - c.x) * (b.y - c.y) - (b.x - c.x) * (a.y - c.y)
        }
        let d1 = sign(pt, self.a, self.b);
        let d2 = sign(pt, self.b, self.c);
        let d3 = sign(pt, self.c, self.a);
        let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
        let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);
        !(has_neg && has_pos)
    }
    */
    fn has_point(&self, pt: Point3<f32>) -> bool {
        let bary = self.cartesian_to_barycentric(pt);
        bary.x >= 0.0
            && bary.x <= 1.0
            && bary.y >= 0.0
            && bary.z <= 1.0
            && bary.z >= 0.0
            && bary.z <= 1.0
    }

    fn bounds(&self) -> [f32; 4] {
        let mut coords_x = [self.a.x, self.b.x, self.c.x];
        let mut coords_y = [self.a.y, self.b.y, self.c.y];
        coords_x.sort_by(|i, j| i.partial_cmp(j).unwrap());
        coords_y.sort_by(|i, j| i.partial_cmp(j).unwrap());
        [coords_x[0], coords_y[0], coords_x[2], coords_y[2]]
    }
    fn cartesian_to_barycentric(&self, pt: Point3<f32>) -> Point3<f32> {
        let v0 = self.b - self.a;
        let v1 = self.c - self.a;
        let v2 = pt - self.a;
        let den = 1.0 / (v0.x * v1.y - v1.x * v0.y);
        let v = (v2.x * v1.y - v1.x * v2.y) * den;
        let w = (v0.x * v2.y - v2.x * v0.y) * den;
        let u = 1.0 - v - w;
        Point3::new(u, v, w)
    }
    fn barycentric_to_cartesian(&self, pt: Point3<f32>) -> Point3<f32> {
        let x = pt.x * self.a.x + pt.y * self.b.x + pt.z * self.c.x;
        let y = pt.x * self.a.y + pt.y * self.b.y + pt.z * self.c.y;
        let z = pt.x * self.a.z + pt.y * self.b.z + pt.z * self.c.z;
        Point3::new(x, y, z)
    }
}

#[derive(Debug, Clone)]
struct Tris3D {
    v_3d: [Point3<f32>; 3],
    v_uv: Tris2D,
    min: Point3<f32>,
    mid: Point3<f32>,
    max: Point3<f32>,
    node_index: usize,
}
impl Bounded for Tris3D {
    fn aabb(&self) -> AABB {
        AABB::with_bounds(self.min, self.max)
    }
}
impl BHShape for Tris3D {
    fn set_bh_node_index(&mut self, index: usize) {
        self.node_index = index;
    }

    fn bh_node_index(&self) -> usize {
        self.node_index
    }
}
impl PartialEq for Tris3D {
    fn eq(&self, other: &Self) -> bool {
        self.node_index == other.node_index
    }
}

#[derive(Debug)]
struct Mesh {
    tris: Vec<Tris3D>,
}

#[derive(Debug, Deserialize)]
struct Coords {
    x: f32,
    y: f32,
    z: f32,
}
#[derive(Debug, Deserialize)]
struct VecCameraJSON {
    data: Vec<CameraJSON>,
}
#[derive(Debug, Deserialize)]
struct CameraJSON {
    location: Coords,
    rotation_euler: Coords,
    fov_x: f32,
    limit_near: f32,
    limit_far: f32,
    image_path: String,
}
#[derive(Debug)]
struct CameraRaw {
    id: usize,
    pos: [f32; 3],
    rot: UnitQuaternion<f32>,
    fov_x: f32,
    limit_near: f32,
    limit_far: f32,
    image_path: String,
}

struct Properties {
    clip_uv: bool,
    fill: bool,
    blending: Blending,
}

fn load_meshes(path_data: &str) -> Vec<Tris3D> {
    let data = obj::Obj::load(Path::new(path_data).join("mesh.obj"))
        .unwrap()
        .data;
    let mut tris = Vec::<Tris3D>::new();
    let mut tris_id: usize = 0;
    for obj in data.objects {
        for group in obj.groups {
            for poly in group.polys {
                let mut tr_min_x = f32::MAX;
                let mut tr_max_x = f32::MIN;
                let mut tr_min_y = f32::MAX;
                let mut tr_max_y = f32::MIN;
                let mut tr_min_z = f32::MAX;
                let mut tr_max_z = f32::MIN;
                let mut vs_pos = Vec::<Point3<f32>>::new();
                let mut vs_uv = Vec::<Point3<f32>>::new();

                for vert in poly.0 {
                    let x = data.position[vert.0][0];
                    let y = data.position[vert.0][1];
                    let z = data.position[vert.0][2];
                    let uv = match vert.1 {
                        Some(i) => match data.texture.get(i) {
                            Some(uv) => uv,
                            _ => continue,
                        },
                        _ => continue,
                    };

                    let u = uv[0];
                    let v = uv[1];
                    vs_pos.push(Point3::new(x, y, z));
                    vs_uv.push(Point3::new(u, v, 0.0));

                    tr_min_x = tr_min_x.min(x);
                    tr_max_x = tr_max_x.max(x);
                    tr_min_y = tr_min_y.min(y);
                    tr_max_y = tr_max_y.max(y);
                    tr_min_z = tr_min_z.min(z);
                    tr_max_z = tr_max_z.max(z);
                }

                if vs_pos.len() >= 3 {
                    let tr_mid_x = (tr_min_x + tr_max_x) / 2.0;
                    let tr_mid_y = (tr_min_y + tr_max_y) / 2.0;
                    let tr_mid_z = (tr_min_z + tr_max_z) / 2.0;
                    tris.push(Tris3D {
                        v_3d: [vs_pos[0], vs_pos[1], vs_pos[2]],
                        v_uv: Tris2D {
                            a: vs_uv[0],
                            b: vs_uv[1],
                            c: vs_uv[2],
                        },
                        min: Point3::new(tr_min_x, tr_min_y, tr_min_z),
                        mid: Point3::new(tr_mid_x, tr_mid_y, tr_mid_z),
                        max: Point3::new(tr_max_x, tr_max_y, tr_max_z),
                        node_index: tris_id,
                    });
                    tris_id += 1;
                }
            }
        }
    }
    return tris;
}

fn load_cameras(path_data: &str) -> Vec<CameraRaw> {
    let file_json = fs::File::open(Path::new(path_data).join("cameras.json")).unwrap();
    let cameras_json: VecCameraJSON = serde_json::from_reader(file_json).unwrap();
    let mut cameras = Vec::<CameraRaw>::new();
    let mut id = 0;
    for cam in cameras_json.data {
        id += 1;
        let pos = [cam.location.x, cam.location.y, cam.location.z];
        let rot = UnitQuaternion::from_euler_angles(
            cam.rotation_euler.x,
            cam.rotation_euler.y,
            cam.rotation_euler.z,
        );
        let fov_x = cam.fov_x;
        let limit_near = cam.limit_near;
        let limit_far = cam.limit_far;
        let image_path = cam.image_path;

        cameras.push(CameraRaw {
            id,
            pos,
            rot,
            fov_x,
            limit_near,
            limit_far,
            image_path,
        });
    }

    cameras
}

fn cast_pixels_rays(
    camera_raw: CameraRaw,
    faces: &Vec<Tris3D>,
    bvh: &BVH,
    mut texture: &mut RgbaImage,
    properties: &Properties,
) {
    let img = image::open(camera_raw.image_path).unwrap();
    let width = img.dimensions().0 as usize;
    let height = img.dimensions().1 as usize;
    let ratio = width as f32 / height as f32;
    let fov_y = 2.0 * ((camera_raw.fov_x / 2.0).tan() / ratio).atan();
    let limit_near = camera_raw.limit_near;
    let limit_far = camera_raw.limit_far;
    let [cam_x, cam_y, cam_z] = camera_raw.pos;
    let rot = camera_raw.rot;
    let cam_tr = Translation3::new(cam_x, cam_y, cam_z);
    let iso = Isometry3::from_parts(cam_tr, rot);
    let perspective = Perspective3::new(ratio, fov_y, limit_near, limit_far);

    for face in faces {
        face_img_to_uv(
            faces,
            bvh,
            &face,
            &iso,
            &perspective,
            &img,
            &mut texture,
            properties,
        );
    }
}

fn _closest_faces(faces: Vec<&Tris3D>, pt: Point3<f32>) -> Vec<&Tris3D> {
    if faces.len() <= 1 {
        return faces;
    }
    let mut closest = Vec::<(f32, &Tris3D)>::new();
    for f in faces {
        closest.push((distance(&f.mid, &pt), f));
    }
    closest.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut range = closest[0]
        .1
        .v_3d
        .iter()
        .map(|&p| distance(&p, &closest[0].1.mid))
        .collect::<Vec<f32>>();
    range.sort_by(|a, b| a.partial_cmp(&b).unwrap());
    let epsilon = range.first().unwrap() * 2.0;

    closest
        .iter()
        .filter(|f| f.0 - closest[0].0 <= epsilon)
        .map(|f| f.1)
        .collect()
}

fn closest_faces(faces: Vec<&Tris3D>, ray: Ray, near: f32, far: f32) -> Vec<&Tris3D> {
    if faces.len() <= 1 {
        return faces;
    }
    let mut closest: Vec<(f32, &Tris3D)> = faces
        .into_iter()
        .map(|f| {
            (
                ray.intersects_triangle(&f.v_3d[0], &f.v_3d[1], &f.v_3d[2])
                    .distance,
                f,
            )
        })
        .collect();

    closest.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let epsilon = near + closest[0].0 / ((far - near).max(0.0001));
    //println!("Epsilon {}", epsilon);

    closest
        .iter()
        .filter(|f| f.0 - closest[0].0 <= epsilon)
        .map(|f| f.1)
        .collect()
}

fn _mix_colors(source: Rgba<u8>, target: &Rgba<u8>) -> Rgba<u8> {
    let sr = source[0];
    let sg = source[1];
    let sb = source[2];
    let tr = target[0];
    let tg = target[1];
    let tb = target[2];
    let ta = target[3];

    if ta == 0 {
        return source;
    } else {
        return Rgba([sr / 2 + tr / 2, sg / 2 + tg / 2, sb / 2 + tb / 2, 255]);
    }
}

fn face_img_to_uv(
    faces: &Vec<Tris3D>,
    bvh: &BVH,
    face: &Tris3D,
    iso: &Isometry3<f32>,
    perspective: &Perspective3<f32>,
    img: &DynamicImage,
    texture: &mut RgbaImage,
    properties: &Properties,
) {
    let clip_uv = properties.clip_uv;
    let uv_width = texture.dimensions().0 as f32;
    let uv_height = texture.dimensions().1 as f32;
    let uv_min_u = (face.v_uv.bounds()[0] * uv_width).floor() as usize;
    let uv_min_v = (face.v_uv.bounds()[1] * uv_height).floor() as usize;
    let uv_max_u = (face.v_uv.bounds()[2] * uv_width).ceil() as usize;
    let uv_max_v = (face.v_uv.bounds()[3] * uv_height).ceil() as usize;

    let cam_width = img.dimensions().0 as f32;
    let cam_height = img.dimensions().1 as f32;

    let face_cam = Tris2D {
        a: perspective.project_point(&iso.inverse_transform_point(&face.v_3d[0])),
        b: perspective.project_point(&iso.inverse_transform_point(&face.v_3d[1])),
        c: perspective.project_point(&iso.inverse_transform_point(&face.v_3d[2])),
    };

    for v in uv_min_v..=uv_max_v {
        for u in uv_min_u..=uv_max_u {
            let p_uv = Point3::new(u as f32 / uv_width as f32, v as f32 / uv_height as f32, 0.0);
            if face.v_uv.has_point(p_uv) {
                let p_bary = face.v_uv.cartesian_to_barycentric(p_uv);
                let p_cam = face_cam.barycentric_to_cartesian(p_bary);

                if face_cam.has_point(p_cam)
                    && p_cam.x >= -1.0
                    && p_cam.y >= -1.0
                    && p_cam.x <= 1.0
                    && p_cam.y <= 1.0
                {
                    let cam_x = (cam_width * (p_cam.x + 1.0) / 2.0) as u32;
                    let cam_y = (cam_height * (p_cam.y + 1.0) / 2.0) as u32;

                    if cam_x < cam_width as u32 && cam_y < cam_height as u32 {
                        if (u as u32) < (uv_width as u32) && (v as u32) < (uv_height as u32)
                            || !clip_uv
                        {
                            let ray_origin_pt = iso.transform_point(
                                &perspective.unproject_point(&Point3::new(p_cam.x, p_cam.y, -1.0)),
                            );

                            let ray_target_pt = iso.transform_point(
                                &perspective.unproject_point(&Point3::new(p_cam.x, p_cam.y, 1.0)),
                            );

                            let ray = Ray::new(
                                ray_origin_pt,
                                Vector3::new(
                                    ray_target_pt.x - ray_origin_pt.x,
                                    ray_target_pt.y - ray_origin_pt.y,
                                    ray_target_pt.z - ray_origin_pt.z,
                                ),
                            );

                            let collisions = closest_faces(
                                bvh.traverse(&ray, &faces),
                                ray,
                                perspective.znear(),
                                perspective.zfar(),
                            );
                            let is_front = collisions.contains(&face); // || collisions.len() == 0;
                            if is_front {
                                let uv_u = match clip_uv {
                                    true => u as u32,
                                    false => (u as f32 % uv_width) as u32,
                                };
                                let uv_v = match clip_uv {
                                    true => v as u32,
                                    false => (v as f32 % uv_height) as u32,
                                };

                                let source_color =
                                    img.get_pixel(cam_x, cam_height as u32 - cam_y - 1);

                                texture.put_pixel(uv_u, uv_height as u32 - uv_v - 1, source_color);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn blend_pixel_with_neigbhours(texture: &RgbaImage, x: u32, y: u32) -> Rgba<u8> {
    let ways = [
        [0, 1],
        [1, 1],
        [1, 0],
        [1, -1],
        [0, -1],
        [-1, 1],
        [-1, 0],
        [-1, -1],
    ];
    let bx = texture.dimensions().0 as i32;
    let by = texture.dimensions().1 as i32;
    let mut neibs_count = 0;
    let mut r = 0 as u32;
    let mut g = 0 as u32;
    let mut b = 0 as u32;

    for way in ways.iter() {
        let col = texture.get_pixel(
            ((x as i32 + way[0] % bx) as u32).min(bx as u32 - 1),
            ((y as i32 + way[1] % by) as u32).min(by as u32 - 1),
        );
        if col[3] != 0 {
            neibs_count += 1;
            r += col[0] as u32;
            g += col[1] as u32;
            b += col[2] as u32;
        }
    }
    if neibs_count != 0 {
        r /= neibs_count;
        g /= neibs_count;
        b /= neibs_count;
    }
    Rgba([r as u8, g as u8, b as u8, 255])
}

enum Blending {
    Average,
    Median,
    Mode,
}

fn average(colors: Vec<[u8; 3]>) -> [u8; 3] {
    let mut sum_r: usize = 0;
    let mut sum_g: usize = 0;
    let mut sum_b: usize = 0;
    colors.iter().for_each(|c| {
        sum_r += c[0] as usize;
        sum_g += c[1] as usize;
        sum_b += c[2] as usize;
    });
    let r = (sum_r / colors.len()) as u8;
    let g = (sum_g / colors.len()) as u8;
    let b = (sum_b / colors.len()) as u8;
    [r, g, b]
}

fn median(colors: &mut Vec<[u8; 3]>) -> [u8; 3] {
    colors.sort_by(|a, b| (col_len(a)).cmp(&col_len(b)));
    colors[colors.len() / 2]
}

fn mode(colors: Vec<[u8; 3]>) -> Vec<[u8; 3]> {
    let mut vec_mode = Vec::new();
    let mut seen_map = HashMap::new();
    let mut max_val = 0;
    for c in colors {
        let ctr = seen_map.entry(c).or_insert(0);
        *ctr += 1;
        if *ctr > max_val {
            max_val = *ctr;
        }
    }
    for (key, val) in seen_map {
        if val == max_val {
            vec_mode.push(key);
        }
    }
    vec_mode
}

fn combine_layers(textures: Vec<RgbaImage>, blending: Blending) -> RgbaImage {
    let (img_res_x, img_res_y) = textures[0].dimensions();
    let mut mono_texture = RgbaImage::new(img_res_x, img_res_y);
    for y in 0..img_res_y {
        for x in 0..img_res_x {
            let mut colors = Vec::<[u8; 3]>::new();
            for part in &textures {
                let col = part.get_pixel(x, y);
                if col[3] != 0 {
                    colors.push([col[0], col[1], col[2]]);
                }
            }
            if colors.len() > 0 {
                let m = match &blending {
                    Blending::Average => average(colors),
                    Blending::Median => median(&mut colors),
                    Blending::Mode => mode(colors)[0],
                };
                mono_texture.put_pixel(x, y, Rgba([m[0], m[1], m[2], 255]))
            }
        }
    }
    mono_texture
}

fn fill_empty_pixels(texture: &mut RgbaImage) {
    let (width, height) = texture.dimensions();
    for v in (0..(height as usize)).rev() {
        for u in 0..(width as usize) {
            let current_color = *texture.get_pixel(u as u32, v as u32);
            if current_color[3] == 0 {
                let blended_color = blend_pixel_with_neigbhours(&texture, u as u32, v as u32);
                if blended_color[3] != 0 {
                    texture.put_pixel(u as u32, v as u32, blended_color)
                }
            }
        }
    }
}

fn col_len(c: &[u8; 3]) -> usize {
    (((c[0] as usize).pow(2) + (c[1] as usize).pow(2) + (c[2] as usize).pow(2)) as f32).sqrt()
        as usize
}

fn main() {
    //CLI
    let args: Vec<_> = env::args().collect();
    if args.len() < 8 {
        println!("Arguments are insufficient. You are allowed to try again.");
        return;
    }
    let path_data = &args[1];
    let path_texture = &args[2];
    let img_res_x = args[3].parse::<u32>().unwrap();
    let img_res_y = args[4].parse::<u32>().unwrap();
    let properties = Properties {
        clip_uv: match args[5].parse::<u8>() {
            Ok(1) => true,
            _ => false,
        },
        fill: match args[6].parse::<u8>() {
            Ok(1) => true,
            _ => false,
        },
        blending: match args[7].parse::<u8>() {
            Ok(0) => Blending::Average,
            Ok(1) => Blending::Median,
            Ok(2) => Blending::Mode,
            _ => Blending::Mode,
        },
    };
    println!("\nRaskrasser welcomes you! Puny humans are instructed to wait..");

    //Loading
    let mut faces: Vec<Tris3D> = load_meshes(path_data);
    println!("OBJ loaded.");
    let cameras = load_cameras(path_data);
    let bvh = BVH::build(&mut faces);
    let cam_num = cameras.len();
    let cameras_loaded = match cam_num {
        1 => "Camera loaded.".to_string(),
        _ => format!("{:?} cameras loaded.", cam_num),
    };
    println!("{}", cameras_loaded);

    //Parallel execution
    let textures: Vec<RgbaImage> = cameras
        .into_par_iter()
        .map(|cam| {
            let mut texture = RgbaImage::new(img_res_x, img_res_y);
            let id = cam.id;
            cast_pixels_rays(cam, &faces, &bvh, &mut texture, &properties);
            println!("Finished cam: #{:?} / {:?}", id, cam_num);
            texture
        })
        .collect();

    //Combining images
    let mut mono_texture = combine_layers(textures, properties.blending);
    //Filling transparent pixels
    if properties.fill {
        fill_empty_pixels(&mut mono_texture);
        println!("Filled empty pixels");
    }
    //Export texture
    // mono_texture = image::imageops::flip_vertical(&mono_texture);
    mono_texture.save(Path::new(path_texture)).unwrap();
    println!("Texture saved!\nRaskrasser out. See you next time.");
}
