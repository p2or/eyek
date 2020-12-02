use bvh::aabb::{Bounded, AABB};
use bvh::bounding_hierarchy::{BHShape, BoundingHierarchy};
use bvh::bvh::BVH;
use bvh::nalgebra::base::Unit;
use bvh::nalgebra::distance;
use bvh::nalgebra::geometry::{Isometry3, Perspective3, Quaternion, Translation3, UnitQuaternion};
use bvh::nalgebra::{Point3, Vector3};
use bvh::ray::Ray;
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use obj;
use serde_derive::Deserialize;
use serde_json;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone)]
struct Tris2D {
    a: Point3<f32>,
    b: Point3<f32>,
    c: Point3<f32>,
}
impl Tris2D {
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
    fn bounds(&self) -> [f32; 4] {
        let mut coords_x = [self.a.x, self.b.x, self.c.x];
        let mut coords_y = [self.a.y, self.b.y, self.c.y];
        coords_x.sort_by(|i, j| i.partial_cmp(j).unwrap());
        coords_y.sort_by(|i, j| i.partial_cmp(j).unwrap());
        //return min_x, min_y, max_x, max_y of triangle
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

#[derive(Debug)]
struct Mesh {
    tris: Vec<Tris3D>,
    //aabb: [[f32; 3]; 2],
}

#[derive(Debug, Deserialize)]
struct VecCameraJSON {
    data: Vec<CameraJSON>,
}
#[derive(Debug, Deserialize)]
struct CameraJSON {
    cameraPosition: Vec<f32>,
    cameraRotation: Vec<f32>,
    imageName: String,
}
#[derive(Debug)]
struct CameraRaw {
    pos: [f32; 3],
    rot: UnitQuaternion<f32>,
    img_path: String,
}

fn load_meshes(path_obj: &str) -> Vec<Tris3D> {
    let data = obj::Obj::load(path_obj).unwrap().data;
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
                    //KOCTbIJIb
                    let z = -data.position[vert.0][2];
                    let u = data.texture[vert.1.unwrap()][0];
                    let v = data.texture[vert.1.unwrap()][1];

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

fn load_cameras(path_json_imgs: &str) -> Vec<CameraRaw> {
    let file_json = fs::File::open(Path::new(path_json_imgs).join("imageData.json")).unwrap();
    let cameras_json: VecCameraJSON = serde_json::from_reader(file_json).unwrap();
    let mut cameras = Vec::<CameraRaw>::new();

    for cam in cameras_json.data {
        let pos = [
            cam.cameraPosition[0],
            cam.cameraPosition[1],
            cam.cameraPosition[2],
        ];

        //This quaternion as a 4D vector of coordinates in the [ x, y, z, w ] storage order.
        let rot = UnitQuaternion::from_quaternion(Quaternion::new(
            //KOCTbIJIb
            //cam.cameraRotation[0],
            cam.cameraRotation[1],
            cam.cameraRotation[2],
            cam.cameraRotation[3],
            cam.cameraRotation[0],
        ));

        let img_path = Path::new(path_json_imgs)
            .join(cam.imageName)
            .to_string_lossy()
            .into_owned();

        cameras.push(CameraRaw { pos, rot, img_path });
    }

    cameras
}

fn project_pixels(
    camera_raw: CameraRaw,
    faces: &Vec<Tris3D>,
    bvh: &BVH,
    mut texture: &mut RgbaImage,
) {
    let img = image::open(camera_raw.img_path).unwrap();
    let width = img.dimensions().0 as usize;
    let height = img.dimensions().1 as usize;
    let ratio = width as f32 / height as f32;
    //let fovy =
    let [cam_x, cam_y, cam_z] = camera_raw.pos;
    let pos_tr = Translation3::new(cam_x, cam_y, cam_z);
    let pos_pt = Point3::new(cam_x, cam_y, cam_z);
    let iso = Isometry3::from_parts(pos_tr, camera_raw.rot);
    let perspective = Perspective3::new(ratio, 3.14 / 2.0, 1.0, 1000.0);
    //let projection = perspective.as_matrix() * iso.to_homogeneous();

    let mut checked_pixels: Vec<Vec<bool>> = Vec::with_capacity(width);
    for _ in 0..width {
        checked_pixels.push(vec![false; height]);
    }

    for y in 0..height {
        for x in 0..width {
            if !checked_pixels[x][y] {
                let ray_target = iso.inverse_transform_point(&perspective.unproject_point(
                    &Point3::new(x as f32 / width as f32, y as f32 / height as f32, 1.0),
                ));
                //  println!("{:?}", ray_target);
                let ray = Ray::new(
                    pos_pt,
                    Vector3::new(ray_target[0], ray_target[1], ray_target[2]),
                );
                let collisions = bvh.traverse(&ray, &faces);
                if collisions.len() == 0 {
                    checked_pixels[x][y] = true;
                    continue;
                }
                let face = closest_face(bvh.traverse(&ray, &faces), pos_pt);
                face_img_to_uv(
                    &face,
                    &iso,
                    &perspective,
                    &mut checked_pixels,
                    &img,
                    &mut texture,
                );
            }
        }
    }
}

fn closest_face(faces: Vec<&Tris3D>, pt: Point3<f32>) -> &Tris3D {
    if faces.len() == 1 {
        return faces[0];
    }
    let mut min_dist = f32::MAX;
    let mut id = 0;
    for (i, face) in faces.iter().enumerate() {
        let dist = distance(&face.mid, &pt);
        if dist < min_dist {
            min_dist = dist;
            id = i;
        }
    }
    faces[id]
}

fn face_img_to_uv(
    face: &Tris3D,
    iso: &Isometry3<f32>,
    perspective: &Perspective3<f32>,
    mut checked_pixels: &mut Vec<Vec<bool>>,
    img: &DynamicImage,
    mut texture: &mut RgbaImage,
) {
}

fn main() {
    let path_obj =
        "/home/p4/Work/Phygitalism/201127_Raskrasser/tests/test_0/Scan/TestScan42Scan.obj";
    let path_json_imgs = "/home/p4/Work/Phygitalism/201127_Raskrasser/tests/test_0";
    let img_res: u32 = 1024;

    let mut faces: Vec<Tris3D> = load_meshes(path_obj);
    let cameras = load_cameras(path_json_imgs);
    let bvh = BVH::build(&mut faces);
    let mut texture = RgbaImage::new(img_res, img_res);
    for cam in cameras {
        project_pixels(cam, &faces, &bvh, &mut texture);
    }
}
