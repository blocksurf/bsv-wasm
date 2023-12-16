pub trait ToHex {
    fn to_hex(&self) -> String;
}

impl ToHex for Vec<u8> {
    fn to_hex(&self) -> String {
        hex_simd::encode_to_string(self, hex_simd::AsciiCase::Lower)
    }
}

impl ToHex for [u8] {
    fn to_hex(&self) -> String {
        hex_simd::encode_to_string(self, hex_simd::AsciiCase::Lower)
    }
}
