use std::{
    io::{Cursor, Read},
    slice::Iter,
    str::FromStr,
    usize,
};

use crate::{
    utils::{from_hex, to_hex},
    OpCodes, Script,
};
use crate::{BSVErrors, VarInt};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_traits::{FromPrimitive, ToPrimitive};

use serde::*;
use strum_macros::Display;

#[derive(Debug, Clone, Display, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScriptBit {
    OpCode(OpCodes),
    If { code: OpCodes, pass: Vec<ScriptBit>, fail: Option<Vec<ScriptBit>> },
    Push(#[serde(serialize_with = "to_hex", deserialize_with = "from_hex")] Vec<u8>),
    PushData(OpCodes, #[serde(serialize_with = "to_hex", deserialize_with = "from_hex")] Vec<u8>),
    Coinbase(#[serde(serialize_with = "to_hex", deserialize_with = "from_hex")] Vec<u8>),
}

impl ScriptBit {
    pub fn inner(&self) -> Option<Vec<u8>> {
        match self {
            ScriptBit::Push(v) => Some(v.to_owned()),
            ScriptBit::PushData(_, v) => Some(v.to_owned()),
            ScriptBit::Coinbase(v) => Some(v.to_owned()),
            _ => None,
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            ScriptBit::OpCode(code) => vec![*code as u8],
            ScriptBit::Push(bytes) => {
                let mut pushbytes = bytes.clone();
                pushbytes.insert(0, bytes.len() as u8);
                pushbytes
            }
            ScriptBit::PushData(code, bytes) => {
                let mut pushbytes = vec![*code as u8];

                let length_bytes = match code {
                    OpCodes::OP_PUSHDATA1 => (bytes.len() as u8).to_le_bytes().to_vec(),
                    OpCodes::OP_PUSHDATA2 => (bytes.len() as u16).to_le_bytes().to_vec(),
                    _ => (bytes.len() as u32).to_le_bytes().to_vec(),
                };
                pushbytes.extend(length_bytes);
                pushbytes.extend(bytes);
                pushbytes
            }
            ScriptBit::If { code, pass, fail } => {
                let mut bytes = vec![*code as u8];

                for bit in pass {
                    bytes.extend_from_slice(&bit.to_vec())
                }

                if let Some(fail) = fail {
                    bytes.push(OpCodes::OP_ELSE as u8);
                    for bit in fail {
                        bytes.extend_from_slice(&bit.to_vec())
                    }
                }
                bytes.push(OpCodes::OP_ENDIF as u8);

                bytes
            }
            ScriptBit::Coinbase(bytes) => bytes.to_vec(),
        }
    }

    pub fn to_asm_string_impl(&self, extended: bool) -> String {
        match self {
            ScriptBit::OpCode(code) => match code {
                v if v.eq(&OpCodes::OP_0) => match extended {
                    true => OpCodes::OP_0.to_string(),
                    false => 0.to_string(),
                },
                _ => code.to_string(),
            },
            ScriptBit::Push(bytes) => match extended {
                true => format!("OP_PUSH {} {}", bytes.len(), hex::encode(bytes)),
                false => hex::encode(bytes),
            },
            ScriptBit::PushData(code, bytes) => match extended {
                true => format!("{} {} {}", code, bytes.len(), hex::encode(bytes)),
                false => hex::encode(bytes),
            },
            ScriptBit::If { code, pass, fail } => {
                let mut string_parts = vec![];

                string_parts.push(code.to_string());

                for bit in pass {
                    let bit_string = bit.to_asm_string_impl(extended);
                    if !bit_string.is_empty() {
                        string_parts.push(bit_string)
                    }
                }

                if let Some(fail) = fail {
                    string_parts.push(OpCodes::OP_ELSE.to_string());
                    for bit in fail {
                        let bit_string = bit.to_asm_string_impl(extended);
                        if !bit_string.is_empty() {
                            string_parts.push(bit_string)
                        }
                    }
                }

                string_parts.push(OpCodes::OP_ENDIF.to_string());

                string_parts.join(" ")
            }
            ScriptBit::Coinbase(bytes) => hex::encode(bytes),
        }
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.to_vec())
    }

    pub fn to_asm_string(&self) -> String {
        self.to_asm_string_impl(false)
    }
    pub fn to_extended_asm_string(&self) -> String {
        self.to_asm_string_impl(true)
    }
}

impl ScriptBit {
    pub fn from_bytes(bytes: &[u8]) -> Result<ScriptBit, BSVErrors> {
        let mut cursor = Cursor::new(bytes);

        if let Ok(byte) = cursor.read_u8() {
            if byte.ne(&(OpCodes::OP_0 as u8)) && byte.lt(&(OpCodes::OP_PUSHDATA1 as u8)) {
                let mut data: Vec<u8> = vec![0; byte as usize];
                match cursor.read(&mut data) {
                    Ok(len) => return Ok(ScriptBit::Push(data[..len].to_vec())),
                    Err(e) => return Err(BSVErrors::DeserialiseScript(format!("Failed to read OP_PUSH data {}", e))),
                }
            }

            match OpCodes::from_u8(byte) {
                Some(v @ (OpCodes::OP_PUSHDATA1 | OpCodes::OP_PUSHDATA2 | OpCodes::OP_PUSHDATA4)) => {
                    let data_length = match v {
                        OpCodes::OP_PUSHDATA1 => cursor.read_u8()? as usize,
                        OpCodes::OP_PUSHDATA2 => cursor.read_u16::<LittleEndian>()? as usize,
                        _ => cursor.read_u32::<LittleEndian>()? as usize,
                    };

                    let mut data = vec![0; data_length];
                    if let Err(e) = cursor.read(&mut data) {
                        return Err(BSVErrors::DeserialiseScript(format!("Failed to read OP_PUSHDATA data {}", e)));
                    }

                    return Ok(ScriptBit::PushData(v, data));
                }
                Some(v) => return Ok(ScriptBit::OpCode(v)),
                None => return Err(BSVErrors::DeserialiseScript(format!("Unknown opcode {}", byte))),
            }
        }

        Err(BSVErrors::DeserialiseScript("Failed to decode ScriptBit".to_string()))
    }
}
