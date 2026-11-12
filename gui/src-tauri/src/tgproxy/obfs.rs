//! Обфускация MTProto (obfuscated2): разбор приветствия клиента с секретом прокси
//! и построение приветствия к серверам Telegram.
//!
//! Схема та же, что в tg-ws-proxy (Flowseal, MIT):
//! клиент → [AES-CTR, ключ = SHA256(prekey + secret)] → прокси →
//! [AES-CTR, ключ из нашего случайного приветствия] → Telegram.

use aes::Aes256;
use ctr::cipher::{KeyIvInit, StreamCipher};
use sha2::{Digest, Sha256};

pub type Ctr = ctr::Ctr128BE<Aes256>;

pub const HANDSHAKE_LEN: usize = 64;
const SKIP_LEN: usize = 8;
const PREKEY_LEN: usize = 32;
const IV_LEN: usize = 16;
const PROTO_TAG_POS: usize = 56;
const DC_IDX_POS: usize = 60;

pub const TAG_ABRIDGED: [u8; 4] = [0xef; 4];
pub const TAG_INTERMEDIATE: [u8; 4] = [0xee; 4];
pub const TAG_SECURE: [u8; 4] = [0xdd; 4];

/// Транспорт MTProto внутри обфускации: от него зависит, как резать поток на пакеты.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proto {
    Abridged,
    Intermediate,
    PaddedIntermediate,
}

impl Proto {
    fn from_tag(tag: &[u8]) -> Option<Proto> {
        match tag {
            t if t == TAG_ABRIDGED => Some(Proto::Abridged),
            t if t == TAG_INTERMEDIATE => Some(Proto::Intermediate),
            t if t == TAG_SECURE => Some(Proto::PaddedIntermediate),
            _ => None,
        }
    }

    pub fn tag(self) -> [u8; 4] {
        match self {
            Proto::Abridged => TAG_ABRIDGED,
            Proto::Intermediate => TAG_INTERMEDIATE,
            Proto::PaddedIntermediate => TAG_SECURE,
        }
    }
}

fn cipher(key: &[u8], iv: &[u8]) -> Ctr {
    Ctr::new(key.into(), iv.into())
}

fn skip64(c: &mut Ctr) {
    let mut z = [0u8; HANDSHAKE_LEN];
    c.apply_keystream(&mut z);
}

/// Четыре шифра одного соединения.
pub struct CryptoCtx {
    /// Расшифровка данных от клиента.
    pub clt_dec: Ctr,
    /// Шифрование данных к клиенту.
    pub clt_enc: Ctr,
    /// Шифрование данных к Telegram.
    pub tg_enc: Ctr,
    /// Расшифровка данных от Telegram.
    pub tg_dec: Ctr,
}

/// Результат разбора приветствия клиента.
pub struct ClientHello {
    /// Номер дата-центра (для тестовых DC — как прислал клиент, 10000+).
    pub dc: i32,
    pub is_media: bool,
    pub proto: Proto,
    /// Расшифровщик клиента, уже прокрученный на 64 байта приветствия.
    clt_dec: Ctr,
    prekey_iv: [u8; PREKEY_LEN + IV_LEN],
}

/// Расшифровывает 64-байтное приветствие клиента. None — неверный секрет или протокол.
pub fn parse_client_hello(hs: &[u8; HANDSHAKE_LEN], secret: &[u8; 16]) -> Option<ClientHello> {
    let mut prekey_iv = [0u8; PREKEY_LEN + IV_LEN];
    prekey_iv.copy_from_slice(&hs[SKIP_LEN..SKIP_LEN + PREKEY_LEN + IV_LEN]);
    let key = Sha256::new().chain_update(&prekey_iv[..PREKEY_LEN]).chain_update(secret).finalize();
    let mut dec = cipher(&key, &prekey_iv[PREKEY_LEN..]);
    let mut plain = *hs;
    dec.apply_keystream(&mut plain);

    let proto = Proto::from_tag(&plain[PROTO_TAG_POS..PROTO_TAG_POS + 4])?;
    let dc_idx = i16::from_le_bytes([plain[DC_IDX_POS], plain[DC_IDX_POS + 1]]) as i32;
    Some(ClientHello { dc: dc_idx.abs(), is_media: dc_idx < 0, proto, clt_dec: dec, prekey_iv })
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("системный генератор случайных чисел недоступен");
    b
}

/// Случайное начало приветствия, которое Telegram не спутает с другим протоколом.
fn acceptable_random(rnd: &[u8; HANDSHAKE_LEN]) -> bool {
    const RESERVED_STARTS: [[u8; 4]; 7] = [
        *b"HEAD",
        *b"POST",
        *b"GET ",
        [0xee; 4],
        [0xdd; 4],
        [0x16, 0x03, 0x01, 0x02],
        *b"OPTI",
    ];
    rnd[0] != 0xef && !RESERVED_STARTS.iter().any(|s| rnd[..4] == s[..]) && rnd[4..8] != [0, 0, 0, 0]
}

/// Приветствие к Telegram для указанного DC (отрицательный — медиа) из заданных случайных байт.
fn relay_init_from(mut rnd: [u8; HANDSHAKE_LEN], proto: Proto, dc_idx: i16, tail_rand: [u8; 2]) -> [u8; HANDSHAKE_LEN] {
    let mut enc = cipher(&rnd[SKIP_LEN..SKIP_LEN + PREKEY_LEN], &rnd[SKIP_LEN + PREKEY_LEN..SKIP_LEN + PREKEY_LEN + IV_LEN]);
    let mut keystream = [0u8; HANDSHAKE_LEN];
    enc.apply_keystream(&mut keystream);
    let mut tail = [0u8; 8];
    tail[..4].copy_from_slice(&proto.tag());
    tail[4..6].copy_from_slice(&dc_idx.to_le_bytes());
    tail[6..].copy_from_slice(&tail_rand);
    for i in 0..8 {
        rnd[PROTO_TAG_POS + i] = tail[i] ^ keystream[PROTO_TAG_POS + i];
    }
    rnd
}

pub fn relay_init(proto: Proto, dc_idx: i16) -> [u8; HANDSHAKE_LEN] {
    let rnd = loop {
        let r = random_bytes::<HANDSHAKE_LEN>();
        if acceptable_random(&r) {
            break r;
        }
    };
    relay_init_from(rnd, proto, dc_idx, random_bytes())
}

/// Собирает шифры соединения из приветствия клиента и нашего приветствия к Telegram.
pub fn crypto_ctx(hello: ClientHello, secret: &[u8; 16], relay_init: &[u8; HANDSHAKE_LEN]) -> CryptoCtx {
    let mut rev = hello.prekey_iv;
    rev.reverse();
    let enc_key = Sha256::new().chain_update(&rev[..PREKEY_LEN]).chain_update(secret).finalize();
    let clt_enc = cipher(&enc_key, &rev[PREKEY_LEN..]);

    let relay = &relay_init[SKIP_LEN..SKIP_LEN + PREKEY_LEN + IV_LEN];
    let mut tg_enc = cipher(&relay[..PREKEY_LEN], &relay[PREKEY_LEN..]);
    skip64(&mut tg_enc);
    let mut relay_rev = [0u8; PREKEY_LEN + IV_LEN];
    relay_rev.copy_from_slice(relay);
    relay_rev.reverse();
    let tg_dec = cipher(&relay_rev[..PREKEY_LEN], &relay_rev[PREKEY_LEN..]);

    CryptoCtx { clt_dec: hello.clt_dec, clt_enc, tg_enc, tg_dec }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Приветствие клиента, как его строит Telegram Desktop (для тестов).
    pub fn client_hello(secret: &[u8; 16], proto: Proto, dc_idx: i16, rnd: [u8; 64]) -> ([u8; 64], Ctr, Ctr) {
        let mut hs = rnd;
        let key = Sha256::new().chain_update(&hs[8..40]).chain_update(secret).finalize();
        let mut enc = cipher(&key, &hs[40..56]);
        let mut rev = [0u8; 48];
        rev.copy_from_slice(&hs[8..56]);
        rev.reverse();
        let dkey = Sha256::new().chain_update(&rev[..32]).chain_update(secret).finalize();
        let dec = cipher(&dkey, &rev[32..]);
        hs[56..60].copy_from_slice(&proto.tag());
        hs[60..62].copy_from_slice(&dc_idx.to_le_bytes());
        let mut encrypted = hs;
        enc.apply_keystream(&mut encrypted);
        hs[..56].copy_from_slice(&rnd[..56]);
        hs[56..].copy_from_slice(&encrypted[56..]);
        (hs, enc, dec)
    }

    fn seq(start: u8) -> [u8; 64] {
        std::array::from_fn(|i| start.wrapping_add((i as u8).wrapping_mul(7)))
    }

    #[test]
    fn parses_client_hello() {
        let secret = [0x11u8; 16];
        let (hs, _, _) = client_hello(&secret, Proto::PaddedIntermediate, -4, seq(3));
        let h = parse_client_hello(&hs, &secret).unwrap();
        assert_eq!((h.dc, h.is_media, h.proto), (4, true, Proto::PaddedIntermediate));
        assert!(parse_client_hello(&hs, &[0x12u8; 16]).is_none());
    }

    /// Эталон посчитан оригинальным tg-ws-proxy (_generate_relay_init с теми же байтами).
    #[test]
    fn relay_init_matches_reference() {
        let r = relay_init_from(seq(1), Proto::Intermediate, 2, [0xaa, 0xbb]);
        assert_eq!(&r[..56], &seq(1)[..56]);
        assert_eq!(hex::encode(&r[56..]), "5e33578fd2adef90");
    }

    /// Данные проходят клиент → прокси → «Telegram» и обратно без искажений.
    #[test]
    fn reencryption_roundtrip() {
        let secret = [0x5au8; 16];
        let (hs, mut c_enc, mut c_dec) = client_hello(&secret, Proto::Abridged, 2, seq(9));
        let hello = parse_client_hello(&hs, &secret).unwrap();
        let init = relay_init(hello.proto, 2);
        let mut ctx = crypto_ctx(hello, &secret, &init);

        // «Telegram» расшифровывает наше приветствие стандартным obfuscated2.
        let mut tg_dec = cipher(&init[8..40], &init[40..56]);
        let mut init_plain = init;
        tg_dec.apply_keystream(&mut init_plain);
        assert_eq!(&init_plain[56..60], &TAG_ABRIDGED);
        assert_eq!(i16::from_le_bytes([init_plain[60], init_plain[61]]), 2);
        let mut rev = [0u8; 48];
        rev.copy_from_slice(&init[8..56]);
        rev.reverse();
        let mut tg_enc = cipher(&rev[..32], &rev[32..]);

        let msg = b"hello telegram, this is a test payload".to_vec();
        let mut up = msg.clone();
        c_enc.apply_keystream(&mut up);
        ctx.clt_dec.apply_keystream(&mut up);
        ctx.tg_enc.apply_keystream(&mut up);
        tg_dec.apply_keystream(&mut up);
        assert_eq!(up, msg);

        let mut down = msg.clone();
        tg_enc.apply_keystream(&mut down);
        ctx.tg_dec.apply_keystream(&mut down);
        ctx.clt_enc.apply_keystream(&mut down);
        c_dec.apply_keystream(&mut down);
        assert_eq!(down, msg);
    }

    #[test]
    fn random_prefix_rules() {
        let mut r = [1u8; 64];
        assert!(acceptable_random(&r));
        r[0] = 0xef;
        assert!(!acceptable_random(&r));
        r[..4].copy_from_slice(b"GET ");
        assert!(!acceptable_random(&r));
        r[..4].copy_from_slice(&[1, 2, 3, 4]);
        r[4..8].copy_from_slice(&[0; 4]);
        assert!(!acceptable_random(&r));
    }
}
