#![allow(unsafe_code)]

use bevy_ecs::{archetype::{Archetype, ArchetypeId}, prelude::*};
use bevy_math::Vec4;
use crate::{Position, Velocity};

pub struct DODArray {
    pub bytes: Vec<u8>,
}

pub struct DODStorage {
    pub pos: DODArray,
    pub vel: DODArray,
}

#[derive(Clone, Copy)]
pub struct FieldInfo {
    pub offset: usize,
    pub size: usize,
}

pub trait DODConvertible: Sized {
    const FIELD_LAYOUT: &'static [FieldInfo];
    fn to_bytes(&self) -> Vec<u8>;
    fn from_bytes(bytes: &[u8]) -> Self;
}

pub fn convert_chunk_to_dod(chunk: &Archetype) -> DODStorage {
    let len = chunk.entities().len();
    use core::mem::size_of;
    let pos_bytes = vec![0u8; len * size_of::<Position>()];
    let vel_bytes = vec![0u8; len * size_of::<Velocity>()];
    DODStorage {
        pos: DODArray { bytes: pos_bytes },
        vel: DODArray { bytes: vel_bytes },
    }
}

pub fn apply_dod_back(_world: &mut World, _storage: &DODStorage, _id: ArchetypeId) {}

pub fn move_simd(storage: &mut DODStorage, dt: f32) {
    move_simd_inner(storage, dt);
}

#[cfg(all(any(target_arch = "x86", target_arch = "x86_64"), target_feature = "sse2"))]
fn move_simd_inner(storage: &mut DODStorage, dt: f32) {
    unsafe {
        use core::arch::x86_64::*;
        let len = storage.pos.bytes.len() / 16;
        let pos_ptr = storage.pos.bytes.as_mut_ptr() as *mut __m128;
        let vel_ptr = storage.vel.bytes.as_mut_ptr() as *mut __m128;
        let dt_vec = _mm_set1_ps(dt);
        for i in 0..len {
            let p = _mm_load_ps(pos_ptr.add(i) as *const f32);
            let v = _mm_load_ps(vel_ptr.add(i) as *const f32);
            let v_dt = _mm_mul_ps(v, dt_vec);
            let res = _mm_add_ps(p, v_dt);
            _mm_store_ps(pos_ptr.add(i) as *mut f32, res);
        }
    }
}

#[cfg(not(all(any(target_arch = "x86", target_arch = "x86_64"), target_feature = "sse2")))]
fn move_simd_inner(storage: &mut DODStorage, dt: f32) {
    let len = storage.pos.bytes.len() / 16;
    for i in 0..len {
        unsafe {
            let pos_ptr = storage.pos.bytes.as_mut_ptr() as *mut Vec4;
            let vel_ptr = storage.vel.bytes.as_ptr() as *const Vec4;
            let pos = &mut *pos_ptr.add(i);
            let vel = &*vel_ptr.add(i);
            *pos = *pos + (*vel * dt);
        }
    }
}
