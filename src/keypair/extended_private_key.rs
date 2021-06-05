use std::{io::{Cursor, Read, Write}, ops::{Add, Rem}, vec};

use bitcoin_hashes::hex::ToHex;
use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
use elliptic_curve::generic_array::typenum::private::IsGreaterOrEqualPrivate;
use k256::{NonZeroScalar, Secp256k1, ecdsa::SigningKey, Scalar, SecretKey};
use primitive_types::{U256, U512};

use anyhow::*;
use getrandom::*;
use snafu::*;
use wasm_bindgen::{prelude::*, throw_str};

use crate::{hash::Hash, PrivateKey, PrivateKeyErrors, PublicKey, PublicKeyErrors};

#[derive(Debug, Snafu)]
pub enum ExtendedPrivateKeyErrors {
  #[snafu(display("Could not generate randomness: {}", error))]
  RandomnessGenerationError { error: anyhow::Error },
  #[snafu(display("Could not calculate private key bytes from seed: {}", error))]
  InvalidSeedHmacError { error: anyhow::Error },
  #[snafu(display("Could not calculate private key: {}", error))]
  InvalidPrivateKeyError { error: PrivateKeyErrors },
  #[snafu(display("Could not calculate public key: {}", error))]
  InvalidPublicKeyError { error: PublicKeyErrors },
  #[snafu(display("Could not serialise xpriv: {}", error))]
  SerialisationError { error: anyhow::Error },

  #[snafu(display("Could not derive xpriv: {}", error))]
  DerivationError { error: anyhow::Error },
}

#[wasm_bindgen]
pub struct ExtendedPrivateKey {
  private_key: PrivateKey,
  public_key: PublicKey,
  chain_code: Vec<u8>,
  depth: u8,
  index: u32,
  parent_fingerprint: Vec<u8>,
}

impl ExtendedPrivateKey {
  pub fn new(
    private_key: &PrivateKey,
    chain_code: &[u8],
    depth: &u8,
    index: &u32,
    parent_fingerprint: Option<&[u8]>,
  ) -> Self {
    let fingerprint = match parent_fingerprint {
      Some(v) => v,
      None => &[0, 0, 0, 0],
    };

    ExtendedPrivateKey {
      private_key: private_key.clone(),
      public_key: PublicKey::from_private_key(private_key, true),
      chain_code: chain_code.to_vec(),
      depth: *depth,
      index: *index,
      parent_fingerprint: fingerprint.to_vec(),
    }
  }

  pub fn to_string_impl(&self) -> Result<String, ExtendedPrivateKeyErrors> {
    let mut serialised = String::new();
    serialised.push_str("0488ade4");
    serialised.push_str(&format!("{:02}", self.depth));

    serialised.push_str(&self.parent_fingerprint.to_hex());

    serialised.push_str(&format!("{:08}", self.index));
    serialised.push_str(&self.chain_code.to_hex());
    serialised.push_str(&format!("00{}", self.private_key.to_hex()));

    let checksum = &match hex::decode(serialised.clone()) {
      Ok(v) => Hash::sha_256d(&v),
      Err(e) => return Err(ExtendedPrivateKeyErrors::SerialisationError { error: anyhow!(e) }),
    }
    .to_bytes()[0..4];
    serialised.push_str(&checksum.to_hex());

    match hex::decode(&serialised) {
      Ok(v) => Ok(bs58::encode(v).into_string()),
      Err(e) => return Err(ExtendedPrivateKeyErrors::SerialisationError { error: anyhow!(e) }),
    }
  }

  pub fn from_string_impl(xprv_string: &str) -> Result<Self> {
    let mut cursor = Cursor::new(bs58::decode(xprv_string).into_vec()?);

    // Skip the first 4 bytes "xprv"
    cursor.set_position(4);

    let depth = cursor.read_u8()?;
    let mut parent_fingerprint = vec![0; 4];
    cursor.read_exact(&mut parent_fingerprint)?;
    let index = cursor.read_u32::<BigEndian>()?;

    let mut chain_code = vec![0; 32];
    cursor.read_exact(&mut chain_code)?;

    // Skip appended 0 byte on private key
    cursor.set_position(cursor.position() + 1);

    let mut private_key_bytes = vec![0; 32];
    cursor.read_exact(&mut private_key_bytes)?;
    let private_key = match PrivateKey::from_bytes_impl(&private_key_bytes) {
      Ok(v) => v,
      Err(e) => return Err(anyhow!(e)),
    };
    let public_key = PublicKey::from_private_key(&private_key, true);

    let mut checksum = vec![0; 4];
    cursor.read_exact(&mut checksum)?;

    Ok(ExtendedPrivateKey {
      private_key,
      public_key,
      chain_code,
      depth,
      index,
      parent_fingerprint,
    })
  }

  pub fn from_random_impl() -> Result<Self, ExtendedPrivateKeyErrors> {
    let mut seed = vec![0; 64];
    match getrandom(&mut seed)  {
      Ok(v) => Ok(v),
      Err(e) => Err(ExtendedPrivateKeyErrors::RandomnessGenerationError{ error: anyhow!(e) }),
    };

    Self::from_seed_impl(seed)
  }

  pub fn from_seed_impl(seed: Vec<u8>) -> Result<Self, ExtendedPrivateKeyErrors> {
    let seed_hmac = Hash::sha_512_hmac(&seed, b"Bitcoin seed");

    let seed_bytes = seed_hmac.to_bytes();
    let mut seed_chunks = seed_bytes.chunks_exact(32 as usize);
    let private_key_bytes = match seed_chunks.next() {
      Some(b) => b,
      None => {
        return Err(ExtendedPrivateKeyErrors::InvalidSeedHmacError {
          error: anyhow!("Could not get 32 bytes for private key"),
        })
      }
    };
    let chain_code = match seed_chunks.next() {
      Some(b) => b,
      None => {
        return Err(ExtendedPrivateKeyErrors::InvalidSeedHmacError {
          error: anyhow!("Could not get 32 bytes for chain code"),
        })
      }
    };

    let priv_key = match PrivateKey::from_bytes_impl(private_key_bytes) {
      Ok(v) => v,
      Err(e) => return Err(ExtendedPrivateKeyErrors::InvalidPrivateKeyError { error: e }),
    };

    let pub_key = PublicKey::from_private_key(&priv_key, true);

    Ok(Self {
      private_key: priv_key.clone(),
      public_key: pub_key.clone(),
      chain_code: chain_code.to_vec(),
      depth: 0,
      index: 0,
      parent_fingerprint: [0, 0, 0, 0].to_vec(),
    })
  }

  pub fn derive_impl(&self, index: u32) -> Result<ExtendedPrivateKey, ExtendedPrivateKeyErrors> {
    let is_hardened = match index {
      v @ 0..=0x7FFFFFFF => false,
      _ => true,
    };

    let key_data = match is_hardened {
      true => {
        let mut bytes: Vec<u8> = vec![];

        bytes.push(0x0);
        bytes.extend_from_slice(&self.private_key.clone().to_bytes());
        bytes.extend_from_slice(&index.clone().to_be_bytes());
        bytes
      }
      false => {
        let mut bytes: Vec<u8> = vec![];

        let mut pub_key_bytes = &match self.public_key.clone().to_bytes_impl() {
          Ok(v) => v,
          Err(e) => return Err(ExtendedPrivateKeyErrors::InvalidPublicKeyError { error: e }),
        };

        bytes.extend_from_slice(&pub_key_bytes);
        bytes.extend_from_slice(&index.clone().to_be_bytes());
        bytes
      }
    };

    let hmac = Hash::sha_512_hmac(&key_data, &self.chain_code.clone());
    let seed_bytes = hmac.to_bytes();

    let mut seed_chunks = seed_bytes.chunks_exact(32 as usize);
    // let mut seed_chunks = seed_bytes.chunks_exact(32 as usize);
    let private_key_bytes = match seed_chunks.next() {
      Some(b) => b,
      None => {
        return Err(ExtendedPrivateKeyErrors::InvalidSeedHmacError {
          error: anyhow!("Could not get 32 bytes for private key"),
        })
      }
    };
    let child_chain_code = match seed_chunks.next() {
      Some(b) => b,
      None => {
        return Err(ExtendedPrivateKeyErrors::InvalidSeedHmacError {
          error: anyhow!("Could not get 32 bytes for chain code"),
        })
      }
    };

    let parent_private_key = SecretKey::from_bytes(self.private_key.clone().to_bytes().as_slice()).unwrap();

    let il = SecretKey::from_bytes(private_key_bytes).unwrap();
    let sclal: Scalar = Scalar::from_bytes_reduced(&il.secret_scalar().to_bytes());

    // child_private_key = il + parent_key % n
    let derived_private_key = parent_private_key.secret_scalar().add(sclal);

    let child_private_key = match PrivateKey::from_bytes_impl(&derived_private_key.to_bytes()) {
      Ok(v) => v,
      Err(e) => return Err(ExtendedPrivateKeyErrors::InvalidPrivateKeyError { error: e }),
    };

    let child_chain_code_bytes = child_chain_code.to_vec();
    let child_pub_key = PublicKey::from_private_key(&child_private_key, true);

    Ok(ExtendedPrivateKey {
      chain_code: child_chain_code_bytes,
      private_key: child_private_key,
      public_key: child_pub_key,
      depth: self.depth + 1,
      index,
      parent_fingerprint: [0, 0, 0, 0].to_vec(),
    })
  }

  pub fn derive_from_path(path: &str) -> Result<ExtendedPrivateKey, ExtendedPrivateKeyErrors> {
    if path.starts_with('m') == false {
      return Err(ExtendedPrivateKeyErrors::DerivationError{ error: anyhow!("Path did not begin with 'm'") });
    }

    let children = path[1..].split('/');

    let child_indices: Vec<u32> = children.map(|x| -> u32 {
      match x.ends_with("'") {
        true => 0 + 2147483648,
        false => 0
      }
    }).collect(); 

    return Err(ExtendedPrivateKeyErrors::DerivationError{ error: anyhow!("Path did not begin with 'm'") });
  }
}

#[wasm_bindgen]
impl ExtendedPrivateKey {
  pub fn get_private_key(&self) -> PrivateKey {
    self.private_key.clone()
  }

  pub fn get_public_key(&self) -> PublicKey {
    self.public_key.clone()
  }

  pub fn get_chain_code(&self) -> Vec<u8> {
    self.chain_code.clone()
  }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl ExtendedPrivateKey {
  pub fn derive(&self, index: u32) -> Result<ExtendedPrivateKey, JsValue> {
    match Self::derive_impl(&self, index) {
      Ok(v) => Ok(v),
      Err(e) => throw_str(&e.to_string()),
    }
  }

  pub fn from_seed(seed: Vec<u8>) -> Result<ExtendedPrivateKey, JsValue> {
    match Self::from_seed_impl(seed) {
      Ok(v) => Ok(v),
      Err(e) => throw_str(&e.to_string()),
    }
  }

  pub fn from_random() -> Result<ExtendedPrivateKey, JsValue> {
    match Self::from_random_impl() {
      Ok(v) => Ok(v),
      Err(e) => throw_str(&e.to_string()),
    }
  }
  pub fn from_string(xprv_string: &str) -> Result<ExtendedPrivateKey, JsValue> {
    match Self::from_string_impl(xprv_string) {
      Ok(v) => Ok(v),
      Err(e) => throw_str(&e.to_string()),
    }
  }
  pub fn to_string(&self) -> Result<String, JsValue> {
    match Self::to_string_impl(&self) {
      Ok(v) => Ok(v),
      Err(e) => throw_str(&e.to_string()),
    }
  }
}

#[cfg(not(target_arch = "wasm32"))]
impl ExtendedPrivateKey {
  pub fn derive(&self, index: u32) -> Result<ExtendedPrivateKey, ExtendedPrivateKeyErrors> {
    Self::derive_impl(&self, index)
  }

  pub fn from_seed(seed: Vec<u8>) -> Result<ExtendedPrivateKey, ExtendedPrivateKeyErrors> {
    Self::from_seed_impl(seed)
  }

  pub fn from_random() -> Result<ExtendedPrivateKey, ExtendedPrivateKeyErrors> {
    Self::from_random_impl()
  }
  pub fn from_string(xprv_string: &str) -> Result<ExtendedPrivateKey> {
    Self::from_string_impl(xprv_string)
  }
  pub fn to_string(&self) -> Result<String, ExtendedPrivateKeyErrors> {
    Self::to_string_impl(&self)
  }
}
