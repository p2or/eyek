use bvh::aabb::{Bounded, AABB};
use bvh::bounding_hierarchy::{BHShape, BoundingHierarchy};
use bvh::bvh::BVH;
use bvh::nalgebra::geometry::{Perspective3, Quaternion};
use bvh::nalgebra::{Point3, Vector3};
use bvh::ray::Ray;
use obj;
use serde_derive::Deserialize;
use serde_json;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone)]
struct Tris {
    v_3d: [Point3<f32>; 3],
    v_uv: [Point3<f32>; 3],
    min: Point3<f32>,
    mid: Point3<f32>,
    max: Point3<f32>,
    node_index: usize,
}
impl Bounded for Tris {
    fn aabb(&self) -> AABB {
        AABB::with_bounds(self.min, self.max)
    }
}
impl BHShape for Tris {
    fn set_bh_node_index(&mut self, index: usize) {
        self.node_index = index;
    }

    fn bh_node_index(&self) -> usize {
        self.node_index
    }
}

#[derive(Debug)]
struct Mesh {
    tris: Vec<Tris>,
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
    pos: Point3<f32>,
    quat_rot: Quaternion<f32>,
    img_path: String,
}

fn load_meshes(path_obj: &str) -> Vec<Tris> {
    let data = obj::Obj::load(path_obj).unwrap().data;
    let mut tris = Vec::<Tris>::new();
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
                    tris.push(Tris {
                        v_3d: [vs_pos[0], vs_pos[1], vs_pos[2]],
                        v_uv: [vs_uv[0], vs_uv[1], vs_uv[2]],
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
        let pos = Point3::new(
            cam.cameraPosition[0],
            cam.cameraPosition[1],
            cam.cameraPosition[2],
        );

        //This quaternion as a 4D vector of coordinates in the [ x, y, z, w ] storage order.
        let quat_rot = Quaternion::new(
            cam.cameraRotation[0],
            cam.cameraRotation[1],
            cam.cameraRotation[2],
            cam.cameraRotation[3],
        );

        let img_path = Path::new(path_json_imgs)
            .join(cam.imageName)
            .to_string_lossy()
            .into_owned();

        cameras.push(CameraRaw {
            pos,
            quat_rot,
            img_path,
        });
    }

    cameras
}

fn main() {
    let path_obj = "/home/p4ymak/Work/Phygitalism/201127_Raskrasser/tests/suz_center.obj";
    let path_json_imgs = "/home/p4ymak/Work/Phygitalism/201109_Projector/from_und3ve10p3d/test_0";
    let mut faces: Vec<Tris> = load_meshes(path_obj);
    let cameras = load_cameras(path_json_imgs);
    let bvh = BVH::build(&mut faces);

    let origin = Point3::new(0.0, 0.0, 0.0);
    let tx = faces[0].v_3d[0][0] / 2.0;
    let ty = faces[0].v_3d[0][1] / 2.0;
    let tz = faces[0].v_3d[0][2] / 2.0;

    let direction = Vector3::new(0.0, 1.0, 0.0);
    let ray = Ray::new(origin, direction);
    //let inter = ray.intersects_triangle(&face.v_3d[0], &face.v_3d[1], &face.v_3d[2]);
    //println!("{:?} {:?} {:?}", inter.distance, inter.u, inter.v);
    println!("Intersects: {:?}", bvh.traverse(&ray, &faces).len());
    println!("{:?}", faces.len());
    println!("{:?}", cameras);
}
