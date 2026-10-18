//! The passphrase rekordbox 6/7 encrypts master.db with. It is the same on every install and
//! stored obfuscated, as pyrekordbox publishes it. Never log or return it through an API.

use std::io::Read;

use crate::{Error, Result};

const XOR: &[u8] = b"657f48f84c437cc1";
const BLOB: &str = "PN_Pq^*N>(JYe*u^8;Yg76HuZ<mR13S?=>)b9;DpoTXV(6ItkU`}8*m6tx_I{Solh_N#dfe{v=";
const ALPHABET: &[u8; 85] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";

pub(crate) fn master_key() -> Result<String> {
    let packed = base85(BLOB)?;
    let plain: Vec<u8> = packed.iter().enumerate().map(|(i, b)| b ^ XOR[i % XOR.len()]).collect();
    let mut text = String::new();
    flate2::read::ZlibDecoder::new(plain.as_slice())
        .read_to_string(&mut text)
        .map_err(|e| Error::Key(e.to_string()))?;
    Ok(text)
}

/// RFC 1924 base85, the alphabet Python's `b85decode` uses. A short last group is padded with the top digit.
fn base85(text: &str) -> Result<Vec<u8>> {
    let mut lookup = [u8::MAX; 256];
    for (value, &digit) in ALPHABET.iter().enumerate() {
        lookup[digit as usize] = value as u8;
    }
    let mut out = Vec::with_capacity(text.len() * 4 / 5);
    for group in text.trim().as_bytes().chunks(5) {
        if group.len() == 1 {
            return Err(Error::Key("truncated base85".into()));
        }
        let mut acc: u64 = 0;
        for place in 0..5 {
            let digit = match group.get(place) {
                Some(&c) if lookup[c as usize] != u8::MAX => u64::from(lookup[c as usize]),
                Some(_) => return Err(Error::Key("bad base85 digit".into())),
                None => 84,
            };
            acc = acc * 85 + digit;
        }
        let word = u32::try_from(acc).map_err(|_| Error::Key("base85 overflow".into()))?;
        out.extend_from_slice(&word.to_be_bytes()[..group.len() - 1]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_to_a_passphrase() {
        let key = master_key().unwrap();
        assert_eq!(key.len(), 64);
        assert!(key.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn base85_matches_known_vectors() {
        assert_eq!(base85("VPX").unwrap(), b"ab".to_vec());
        assert_eq!(base85("Xk~0{Zy<MXa%^M").unwrap(), b"hello world".to_vec());
    }
}
