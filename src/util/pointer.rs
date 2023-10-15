use std::{io::Read, fmt::Debug, ops::{Add, Sub}};

use anyhow::Result;
use byteorder::{LittleEndian, ReadBytesExt};

macro_rules! from_type {
	($t:ident, $from:ty) => {
		impl From<$from> for $t {
			fn from(value: $from) -> Self {
				Pointer(value.into())
			}
		}
		
		impl Add<$from> for $t {
			type Output = Self;
		
			fn add(self, rhs: $from) -> Self {
				$t(self.0 + u32::from(rhs))
			}
		}
		
		impl Sub<$from> for $t {
			type Output = Self;
		
			fn sub(self, rhs: $from) -> Self {
				$t(self.0 - u32::from(rhs))
			}
		}
	};
}

macro_rules! from_type_unwrap {
	($t:ident, $from:ty) => {
		impl From<$from> for $t {
			fn from(value: $from) -> Self {
				Pointer(value.try_into().unwrap())
			}
		}
		
		impl Add<$from> for $t {
			type Output = Self;
		
			fn add(self, rhs: $from) -> Self {
				// it's beautiful
				$t((i32::try_from(self.0).unwrap() + i32::try_from(rhs).unwrap()).try_into().unwrap())
			}
		}
		
		impl Sub<$from> for $t {
			type Output = Self;
		
			fn sub(self, rhs: $from) -> Self {
				$t((i32::try_from(self.0).unwrap() - i32::try_from(rhs).unwrap()).try_into().unwrap())
			}
		}
	};
}

macro_rules! into_type {
	($t:ident, $into:ty) => {
		impl From<$t> for $into {
			fn from(value: $t) -> Self {
				value.0.into()
			}
		}
	};
}

macro_rules! into_type_unwrap {
	($t:ident, $into:ty) => {
		impl From<$t> for $into {
			fn from(value: $t) -> Self {
				value.0.try_into().unwrap()
			}
		}
	};
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pointer(pub u32);

impl Pointer {
	pub fn read(reader: &mut impl Read) -> Result<Option<Pointer>> {
		let value = reader.read_u32::<LittleEndian>()?;
        
		if value != 0 {
            Ok(Some(Pointer(value)))
        } else {
            Ok(None)
        }
	}
}

impl Debug for Pointer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Pointer({:#x})", self.0))
    }
}

impl Add<Self> for Pointer {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Pointer(self.0 + rhs.0)
    }
}

impl Sub<Self> for Pointer {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Pointer(self.0 - rhs.0)
    }
}

from_type!(Pointer, u32);

from_type_unwrap!(Pointer, i32);
from_type_unwrap!(Pointer, u64);
from_type_unwrap!(Pointer, i64);
from_type_unwrap!(Pointer, usize);

into_type!(Pointer, u32);
into_type!(Pointer, u64);
into_type!(Pointer, i64);

into_type_unwrap!(Pointer, i32);
into_type_unwrap!(Pointer, usize);

// TODO: are these used anywhere? if so, write these using macros too
impl Into<u32> for &Pointer {
    fn into(self) -> u32 {
        self.0
    }
}

impl Into<usize> for &Pointer {
    fn into(self) -> usize {
        self.0.try_into().unwrap()
    }
}
