use std::io::{Read, Seek, Write};

use binrw::{BinRead, BinResult, BinWrite, Endian};
use na::{ArrayStorage, Const, Matrix, U3};

#[derive(Clone, Copy, Debug, PartialEq, Default, BinRead, BinWrite)]
#[brw(little)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z }
    }
    
    pub fn to_na(&self) -> na::Vec3 {
        na::Vec3::new(self.x, self.y, self.z)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, BinRead, BinWrite)]
#[brw(little)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Vec4 { x, y, z, w }
    }
    
    pub fn to_na(&self) -> na::Vec4 {
        na::Vec4::new(self.x, self.y, self.z, self.w)
    }
}

// why doesn't this exist by default
pub type Matrix3x3<T> = Matrix<T, U3, U3, ArrayStorage<T, 3, 3>>;

// binrw matrix helper
pub struct SerializableMatrix<const R: usize, const C: usize> {
    data: ArrayStorage<f32, R, C>,
}

impl<const R: usize, const C: usize> Into<Matrix<f32, Const<R>, Const<C>, ArrayStorage<f32, R, C>>> for SerializableMatrix<R, C> {
    fn into(self) -> Matrix<f32, Const<R>, Const<C>, ArrayStorage<f32, R, C>> {
        Matrix::<f32, Const<R>, Const<C>, ArrayStorage<f32, R, C>>::from_array_storage(self.data)
    }
}

impl<const R: usize, const C: usize> From<&Matrix<f32, Const<R>, Const<C>, ArrayStorage<f32, R, C>>> for SerializableMatrix<R, C> {
    fn from(value: &Matrix<f32, Const<R>, Const<C>, ArrayStorage<f32, R, C>>) -> Self {
        SerializableMatrix {
            data: value.data.clone(),
        }
    }
}

impl<const R: usize, const C: usize> BinRead for SerializableMatrix<R, C> {
    type Args<'a> = ();

    fn read_options<T: Read + Seek>(reader: &mut T, endian: Endian, _: Self::Args<'_>) -> BinResult<Self> {
        let numbers_result: BinResult<Vec<f32>> = (0..R * C)
            .map(|_| f32::read_options(reader, endian, ()))
            .collect();
        let numbers = numbers_result?;
        
        Ok(Self {
            data: Matrix::<f32, Const<R>, Const<C>, ArrayStorage<f32, R, C>>::from_row_slice(&numbers).data,
        })
    }
}

impl<const R: usize, const C: usize> BinWrite for SerializableMatrix<R, C> {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(&self, writer: &mut W, endian: Endian, _: Self::Args<'_>) -> BinResult<()> {
        self.data.as_slice().write_options(writer, endian, ())
    }
}
