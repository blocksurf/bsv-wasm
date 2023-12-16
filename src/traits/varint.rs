use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use std::io::Cursor;
use std::io::Read;
use std::io::Result;
use std::ops::Add;
use std::ops::BitAnd;
use std::ops::BitOr;
use std::ops::Shl;

use crate::OpCodes;

pub trait VarIntReader {
    fn read_varint(&mut self) -> Result<u64>;
}

pub trait VarIntWriter {
    fn write_varint(&mut self, varint: u64) -> std::io::Result<usize>;
}

pub struct VarInt {}

impl VarInt {
    pub fn get_varint_size(data_length: u64) -> usize {
        if data_length <= 252 {
            1
        } else if data_length <= 0xffff {
            2
        } else if data_length <= 0xffffffff {
            4
        } else {
            8
        }
    }

    pub fn get_pushdata_opcode(length: u64) -> Option<OpCodes> {
        if length <= 0x4b {
            None
        } else if length <= 0xff {
            Some(OpCodes::OP_PUSHDATA1)
        } else if length <= 0xffff {
            Some(OpCodes::OP_PUSHDATA2)
        } else {
            Some(OpCodes::OP_PUSHDATA4)
        }
    }

    pub fn get_varint_bytes(length: u64) -> Vec<u8> {
        if length <= 252 {
            vec![length as u8]
        } else if length <= 0xff {
            let mut push1 = vec![0xfd];
            push1.extend((length as u16).to_le_bytes());
            push1
        } else if length <= 0xffff {
            let mut push2 = vec![0xfe];
            push2.extend((length as u32).to_le_bytes());
            push2
        } else {
            let mut push4 = vec![0xff];
            push4.extend(length.to_le_bytes());
            push4
        }
    }
}

impl VarIntReader for Cursor<Vec<u8>> {
    fn read_varint(&mut self) -> Result<u64> {
        match self.read_u8() {
            Ok(0xff) => self.read_u64::<LittleEndian>(),
            Ok(0xfe) => self.read_u32::<LittleEndian>().map(|x| x as u64),
            Ok(0xfd) => self.read_u16::<LittleEndian>().map(|x| x as u64),
            Ok(v) => Ok(v as u64),
            Err(e) => Err(e),
        }
    }
}

impl VarIntWriter for Cursor<Vec<u8>> {
    /**
     * Borrowed from rust-sv by Brenton Gunning
     */
    fn write_varint(&mut self, varint: u64) -> Result<usize> {
        let mut write = || {
            if varint <= 252 {
                self.write_u8(varint as u8)
            } else if varint <= 0xffff {
                self.write_u8(0xfd).and_then(|_| self.write_u16::<LittleEndian>(varint as u16))
            } else if varint <= 0xffffffff {
                self.write_u8(0xfe).and_then(|_| self.write_u32::<LittleEndian>(varint as u32))
            } else {
                self.write_u8(0xff).and_then(|_| self.write_u64::<LittleEndian>(varint))
            }
        };

        write()?;
        Ok(varint as usize)
    }
}

impl VarIntReader for Vec<u8> {
    fn read_varint(&mut self) -> Result<u64> {
        let mut cursor = Cursor::new(&self);
        VarIntUtil::read_var_int(&mut cursor)
    }
}

impl VarIntWriter for Vec<u8> {
    /**
     * Borrowed from rust-sv by Brenton Gunning
     */
    fn write_varint(&mut self, varint: u64) -> Result<usize> {
        let mut write = || {
            if varint <= 252 {
                self.write_u8(varint as u8)
            } else if varint <= 0xffff {
                self.write_u8(0xfd).and_then(|_| self.write_u16::<LittleEndian>(varint as u16))
            } else if varint <= 0xffffffff {
                self.write_u8(0xfe).and_then(|_| self.write_u32::<LittleEndian>(varint as u32))
            } else {
                self.write_u8(0xff).and_then(|_| self.write_u64::<LittleEndian>(varint))
            }
        };

        write()?;
        Ok(varint as usize)
    }
}

use std::io::{Error, ErrorKind};

#[inline]
/// Checks if the nth bit is 1
pub fn is_bit_set(v: u8, bit_index: u8) -> bool {
    v & (1 << bit_index) != 0
}

#[inline]
// Is the termination byte in the varint slice
pub fn is_last_byte(b: u8) -> bool {
    b & 0x80 == 0
}

pub struct VarIntUtil;

impl VarIntUtil {
    #[inline]
    pub fn read_var_int<S: Read, I>(cursor: &mut S) -> Result<I>
    where
        I: From<u8> + From<u64> + Shl<Output = I> + BitOr<Output = I> + BitAnd<Output = I> + Add<Output = I> + PartialOrd,
    {
        let mut n: I = I::from(0u8);

        let overflow = I::from(std::u64::MAX >> 7);

        let max_size = (std::mem::size_of::<I>() * 8 + 6) / 7;
        for _ in 0..max_size {
            if n > overflow {
                return Err(Error::new(ErrorKind::InvalidData, "read_var_int overflow"));
            }

            let ch_data = Self::read_u8(cursor)?;
            n = (n << I::from(7u8)) | (I::from(ch_data) & I::from(0x7Fu8));
            if (ch_data & 0x80) == 0 {
                return Ok(n);
            }
            n = n + I::from(1u8);
        }

        Err(Error::new(ErrorKind::InvalidData, "deserialization error"))
    }

    #[inline]
    pub fn read_u8<S: Read>(s: &mut S) -> Result<u8> {
        let mut buffer = [0u8; 1];
        s.read_exact(&mut buffer)?;
        Ok(buffer[0])
    }
    #[inline]
    pub fn read_u16<S: Read>(s: &mut S) -> Result<u16> {
        let mut buffer = [0u8; 2];
        s.read_exact(&mut buffer)?;
        Ok(u16::from_le_bytes(buffer))
    }
    #[inline]
    pub fn read_u32<S: Read>(s: &mut S) -> Result<u32> {
        let mut buffer = [0u8; 4];
        s.read_exact(&mut buffer)?;
        Ok(u32::from_le_bytes(buffer))
    }
    #[inline]
    pub fn read_u64<S: Read>(s: &mut S) -> Result<u64> {
        let mut buffer = [0u8; 8];
        s.read_exact(&mut buffer)?;
        Ok(u64::from_le_bytes(buffer))
    }
    #[inline]
    pub fn read_u256_slice<S: Read>(s: &mut S) -> Result<[u8; 32]> {
        let mut buffer = [0u8; 32];
        s.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    //
    // Old methods
    //

    pub fn get_bytes(length: u64) -> Vec<u8> {
        if length <= 252 {
            vec![length as u8]
        } else if length <= 0xff {
            let mut push1 = vec![0xfd];
            push1.extend((length as u16).to_le_bytes());
            push1
        } else if length <= 0xffff {
            let mut push2 = vec![0xfe];
            push2.extend((length as u32).to_le_bytes());
            push2
        } else {
            let mut push4 = vec![0xff];
            push4.extend(length.to_le_bytes());
            push4
        }
    }
    /// Reads compact bytes & return (value, size)
    pub fn read_bytes(bytes: Vec<u8>) -> (u64, u8) {
        match bytes[0] {
            0xff => (u64::from_le_bytes(bytes[1..9].try_into().unwrap()), 9),
            0xfe => (u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as u64, 5),
            0xfd => (u16::from_le_bytes(bytes[1..3].try_into().unwrap()) as u64, 3),
            _ => (u8::from_le_bytes([bytes[0]]) as u64, 1),
        }
    }

    pub fn decompress_value(input: u64) -> u64 {
        if input == 0 {
            return 0;
        }

        let mut n;
        let mut x = input - 1;
        let mut e = x % 10;
        x /= 10;

        if e < 9 {
            let d = (x % 9) + 1;
            x /= 9;
            n = x * 10 + d;
        } else {
            n = x + 1;
        }

        while e > 0 {
            n *= 10;
            e -= 1;
        }

        n
    }
}

impl VarIntReader for Cursor<&'_ [u8]> {
    fn read_varint(&mut self) -> Result<u64> {
        VarIntUtil::read_var_int(self)
    }
}
